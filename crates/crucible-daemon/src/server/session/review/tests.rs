use super::*;
use crate::test_support::temp_session_manager;
use crucible_core::protocol::RequestId;
use crucible_core::session::{GateBlock, PhysicalRoot, TreeSha};
use tempfile::TempDir;

// ── End-to-end fixture: real git worktree, real ledger, real handlers ───

/// A session whose ledger tracks a one-file git repo, plus the managers
/// the handlers need. Held together because `TempDir` must outlive the
/// ledger that points at it.
struct Fixture {
    dir: TempDir,
    am: Arc<AgentManager>,
    sm: Arc<SessionManager>,
    event_tx: broadcast::Sender<SessionEventMessage>,
    events: broadcast::Receiver<SessionEventMessage>,
    session: String,
}

use crate::test_support::git;

impl Fixture {
    async fn new(initial: &str) -> Self {
        use crate::agent_manager::AgentManagerParams;
        use crate::background_manager::BackgroundJobManager;
        use crate::kiln_manager::KilnManager;

        let dir = TempDir::new().unwrap();
        git(dir.path(), &["init", "-q"]).await;
        git(dir.path(), &["config", "user.email", "t@t"]).await;
        git(dir.path(), &["config", "user.name", "t"]).await;
        std::fs::write(dir.path().join("a.txt"), initial).unwrap();
        git(dir.path(), &["add", "."]).await;
        git(dir.path(), &["commit", "-q", "-m", "init"]).await;

        let (event_tx, events) = broadcast::channel(64);
        let kiln_manager = Arc::new(KilnManager::new());
        let session_manager = temp_session_manager();
        let am = Arc::new(AgentManager::new(AgentManagerParams {
            kiln_manager,
            session_manager: session_manager.clone(),
            background_manager: Arc::new(BackgroundJobManager::new(event_tx.clone())),
            mcp_gateway: None,
            llm_config: None,
            acp_config: None,
            context_config: None,
            permission_config: None,
            plugin_loader: None,
            card_roots: Default::default(),
        }));

        Self {
            dir,
            am,
            sm: session_manager,
            event_tx,
            events,
            session: "sess".to_string(),
        }
    }

    /// Open the ledger the way a turn would.
    async fn open_ledger(&self) {
        self.am
            .review
            .open(&self.session, &[self.dir.path().to_path_buf()])
            .await
            .unwrap();
    }

    /// One bracketed "tool call" that rewrites the tracked file.
    async fn call(&self, tool_call_id: &str, contents: &str) {
        let handle = self.am.review.open_bracket(&self.session).await.unwrap();
        std::fs::write(self.dir.path().join("a.txt"), contents).unwrap();
        self.am
            .review
            .close(&self.session, handle, tool_call_id, 1)
            .await
            .unwrap();
    }

    fn read(&self) -> String {
        std::fs::read_to_string(self.dir.path().join("a.txt")).unwrap()
    }

    fn request(&self, method: &str, mut params: serde_json::Value) -> Request {
        params["session_id"] = serde_json::json!(self.session);
        Request {
            jsonrpc: "2.0".to_string(),
            id: Some(RequestId::Number(1)),
            method: method.to_string(),
            params,
        }
    }

    async fn list(&self) -> Vec<ComposedHunk> {
        let resp = handle_review_list_hunks(
            self.request("review.list_hunks", serde_json::json!({})),
            &self.am,
            &self.sm,
        )
        .await;
        serde_json::from_value(resp.result.expect("hunks")["hunks"].clone()).unwrap()
    }

    /// Drain the event channel and report which `review_changed` reasons
    /// arrived.
    fn review_reasons(&mut self) -> Vec<String> {
        let mut reasons = Vec::new();
        while let Ok(evt) = self.events.try_recv() {
            if evt.event == "review_changed" {
                reasons.push(evt.data["reason"].as_str().unwrap_or_default().to_string());
            }
        }
        reasons
    }
}

/// A session that never ran a turn has no ledger. That is an empty queue,
/// not a broken daemon — the panel opens on every session, including ones
/// that have not been sent a message yet.
#[tokio::test]
async fn listing_a_session_with_no_ledger_is_an_empty_queue() {
    let fx = Fixture::new("one\n").await;
    let resp = handle_review_list_hunks(
        fx.request("review.list_hunks", serde_json::json!({})),
        &fx.am,
        &fx.sm,
    )
    .await;
    let result = resp.result.expect("success");
    assert_eq!(result["hunks"].as_array().unwrap().len(), 0);
    assert_eq!(result["comments"].as_array().unwrap().len(), 0);
}

/// The listing is how a gate block survives a reload. `review_gate` is an
/// event, and events are dropped rather than replayed across a reconnect, so a
/// tab opened while a turn is already parked has no other way to learn the
/// agent is waiting rather than hung.
#[tokio::test]
async fn listing_reports_the_block_a_parked_turn_is_waiting_on() {
    let fx = Fixture::new("one\n").await;
    let _hold = fx.am.review.hold_gate(
        &fx.session,
        GateBlock {
            tool: "edit_file".to_string(),
            path: "a.txt".to_string(),
        },
    );

    let resp = handle_review_list_hunks(
        fx.request("review.list_hunks", serde_json::json!({})),
        &fx.am,
        &fx.sm,
    )
    .await;
    let result = resp.result.expect("success");
    assert_eq!(result["gate"]["tool"], "edit_file");
    assert_eq!(result["gate"]["path"], "a.txt");
}

/// `null` and a missing key mean different things to a client: `null` clears a
/// stale chip, an absent key means the daemon does not report gate state and
/// whatever the event stream established must stand. This daemon reports it, so
/// the key is always there.
#[tokio::test]
async fn listing_reports_an_explicit_null_when_no_turn_is_parked() {
    let fx = Fixture::new("one\n").await;
    let resp = handle_review_list_hunks(
        fx.request("review.list_hunks", serde_json::json!({})),
        &fx.am,
        &fx.sm,
    )
    .await;
    let result = resp.result.expect("success");
    assert!(
        result.get("gate").is_some_and(serde_json::Value::is_null),
        "an unparked session must answer null, not omit the key: {result}"
    );
}

/// The block is a live fact about a turn, not a decision, so it must not
/// outlive the hold. A listing that kept reporting it would be the permanent
/// visible lie `GateHold`'s `Drop` exists to prevent, one layer further out.
#[tokio::test]
async fn listing_stops_reporting_a_block_once_the_hold_is_released() {
    let fx = Fixture::new("one\n").await;
    let hold = fx.am.review.hold_gate(
        &fx.session,
        GateBlock {
            tool: "edit_file".to_string(),
            path: "a.txt".to_string(),
        },
    );
    drop(hold);

    let resp = handle_review_list_hunks(
        fx.request("review.list_hunks", serde_json::json!({})),
        &fx.am,
        &fx.sm,
    )
    .await;
    assert!(resp.result.expect("success")["gate"].is_null());
}

#[tokio::test]
async fn accepting_a_hunk_records_the_state_and_emits_review_changed() {
    let mut fx = Fixture::new("one\n").await;
    fx.open_ledger().await;
    fx.call("call-1", "two\n").await;
    let _ = fx.review_reasons();

    let hunk = fx.list().await.into_iter().next().expect("a hunk");
    let resp = handle_review_set_state(
        fx.request(
            "review.set_state",
            serde_json::json!({ "hunk_id": hunk.id, "state": "accepted" }),
        ),
        &fx.am,
        &fx.sm,
        &fx.event_tx,
    )
    .await;
    assert!(resp.error.is_none(), "{:?}", resp.error);

    assert_eq!(fx.list().await[0].state, ReviewState::Accepted);
    assert_eq!(fx.review_reasons(), vec!["accepted".to_string()]);
    // Accepting must not touch the worktree.
    assert_eq!(fx.read(), "two\n");
}

/// Reject writes to disk immediately — §3 rules out a deferred mark,
/// because the agent keeps editing while the mark waits.
#[tokio::test]
async fn rejecting_a_hunk_reverts_the_file_and_drains_the_queue() {
    let mut fx = Fixture::new("one\n").await;
    fx.open_ledger().await;
    fx.call("call-1", "two\n").await;
    let _ = fx.review_reasons();

    let hunk = fx.list().await.into_iter().next().expect("a hunk");
    let resp = handle_review_set_state(
        fx.request(
            "review.set_state",
            serde_json::json!({ "hunk_id": hunk.id, "state": "rejected" }),
        ),
        &fx.am,
        &fx.sm,
        &fx.event_tx,
    )
    .await;
    assert!(resp.error.is_none(), "{:?}", resp.error);

    assert_eq!(fx.read(), "one\n");
    assert!(fx.list().await.is_empty(), "reverted hunk still listed");
    assert_eq!(fx.review_reasons(), vec!["rejected".to_string()]);
}

/// Reverting an unattributed hunk would destroy a human's own edit while
/// reporting that an agent edit was undone. The client gets INVALID_PARAMS
/// so it can drop the affordance rather than retrying.
#[tokio::test]
async fn an_external_hunk_cannot_be_reverted() {
    let fx = Fixture::new("one\n").await;
    fx.open_ledger().await;
    // No bracket: this is the user typing in their own editor.
    std::fs::write(fx.dir.path().join("a.txt"), "typed by hand\n").unwrap();

    let hunk = fx.list().await.into_iter().next().expect("a hunk");
    assert!(hunk.is_external());

    let resp = handle_review_set_state(
        fx.request(
            "review.set_state",
            serde_json::json!({ "hunk_id": hunk.id, "state": "rejected" }),
        ),
        &fx.am,
        &fx.sm,
        &fx.event_tx,
    )
    .await;
    assert_eq!(resp.error.expect("refused").code, INVALID_PARAMS);
    assert_eq!(fx.read(), "typed by hand\n");
}

/// A stale client acting on an identity the ledger no longer knows must be
/// told to re-list, never have its decision land on whatever occupies
/// those lines now.
#[tokio::test]
async fn deciding_on_an_unknown_hunk_is_refused() {
    let fx = Fixture::new("one\n").await;
    fx.open_ledger().await;
    fx.call("call-1", "two\n").await;

    let resp = handle_review_set_state(
        fx.request(
            "review.set_state",
            serde_json::json!({ "hunk_id": "0000", "state": "accepted" }),
        ),
        &fx.am,
        &fx.sm,
        &fx.event_tx,
    )
    .await;
    assert_eq!(resp.error.expect("refused").code, INVALID_PARAMS);
}

#[tokio::test]
async fn an_unparseable_state_is_refused_before_the_ledger_is_touched() {
    let fx = Fixture::new("one\n").await;
    let resp = handle_review_set_state(
        fx.request(
            "review.set_state",
            serde_json::json!({ "hunk_id": "0000", "state": "Accepted" }),
        ),
        &fx.am,
        &fx.sm,
        &fx.event_tx,
    )
    .await;
    let err = resp.error.expect("refused");
    assert_eq!(err.code, INVALID_PARAMS);
    assert!(err.message.contains("Invalid 'state'"), "{}", err.message);
}

/// A bulk decision is one daemon call that applies in the given order and
/// names each hunk it refused. The refusals do not stop the loop: a later
/// hunk's range is independent of an earlier one's identity, so the client
/// learns exactly which of its ids were stale instead of losing the whole
/// batch to the first one.
#[tokio::test]
async fn a_bulk_reject_applies_in_order_and_reports_the_hunk_it_no_longer_knows() {
    let mut fx = Fixture::new("1\n2\n3\n4\n5\n6\n7\n8\n9\n").await;
    fx.open_ledger().await;
    fx.call("call-1", "one\n2\n3\n4\nfive\n6\n7\n8\nnine\n")
        .await;
    let _ = fx.review_reasons();

    let mut hunks = fx.list().await;
    hunks.sort_by_key(|h| h.current_range.start);
    assert_eq!(hunks.len(), 3, "{hunks:?}");
    let ids: Vec<HunkId> = hunks.iter().map(|h| h.id.clone()).collect();

    // The file moves under the second hunk. `HunkId::derive` hashes `after`,
    // so the id the client holds names a hunk the ledger no longer knows.
    std::fs::write(
        fx.dir.path().join("a.txt"),
        "one\n2\n3\n4\nFIVE\n6\n7\n8\nnine\n",
    )
    .unwrap();

    let resp = handle_review_set_states(
        fx.request(
            "review.set_states",
            serde_json::json!({ "hunk_ids": ids, "state": "rejected" }),
        ),
        &fx.am,
        &fx.sm,
        &fx.event_tx,
    )
    .await;
    let result = resp.result.expect("a bulk decision answers success");

    assert_eq!(fx.read(), "1\n2\n3\n4\nFIVE\n6\n7\n8\n9\n");
    assert_eq!(result["state"], serde_json::json!("rejected"));
    assert_eq!(
        result["applied"],
        serde_json::json!([ids[0], ids[2]]),
        "the first and third revert, in the given order"
    );
    let failed = result["failed"].as_array().expect("failed is a list");
    assert_eq!(failed.len(), 1, "{failed:?}");
    assert_eq!(failed[0]["hunk_id"], serde_json::json!(ids[1]));
    let reason = failed[0]["reason"].as_str().unwrap_or_default();
    assert!(
        reason.contains("unknown hunk"),
        "the refusal names the cause: {reason}"
    );
    // One decision, one event: the panel redraws once, not per hunk.
    assert_eq!(fx.review_reasons(), vec!["rejected".to_string()]);
}

/// Accepting in bulk touches no file and records every state.
#[tokio::test]
async fn a_bulk_accept_records_every_state_and_emits_one_review_changed() {
    let mut fx = Fixture::new("1\n2\n3\n").await;
    fx.open_ledger().await;
    fx.call("call-1", "one\n2\nthree\n").await;
    let _ = fx.review_reasons();

    let ids: Vec<HunkId> = fx.list().await.into_iter().map(|h| h.id).collect();
    assert_eq!(ids.len(), 2);

    let resp = handle_review_set_states(
        fx.request(
            "review.set_states",
            serde_json::json!({ "hunk_ids": ids, "state": "accepted" }),
        ),
        &fx.am,
        &fx.sm,
        &fx.event_tx,
    )
    .await;
    let result = resp.result.expect("success");
    assert_eq!(result["applied"].as_array().unwrap().len(), 2);
    assert_eq!(result["failed"], serde_json::json!([]));

    assert!(fx
        .list()
        .await
        .iter()
        .all(|h| h.state == ReviewState::Accepted));
    assert_eq!(fx.read(), "one\n2\nthree\n");
    assert_eq!(fx.review_reasons(), vec!["accepted".to_string()]);
}

/// A bulk decision that applied nothing changed nothing, so the panel is not
/// told to redraw.
#[tokio::test]
async fn a_bulk_decision_that_applies_nothing_emits_no_review_changed() {
    let mut fx = Fixture::new("one\n").await;
    fx.open_ledger().await;
    fx.call("call-1", "two\n").await;
    let _ = fx.review_reasons();

    let resp = handle_review_set_states(
        fx.request(
            "review.set_states",
            serde_json::json!({ "hunk_ids": ["0000"], "state": "accepted" }),
        ),
        &fx.am,
        &fx.sm,
        &fx.event_tx,
    )
    .await;
    let result = resp
        .result
        .expect("a refused hunk is a report, not an error");
    assert_eq!(result["applied"], serde_json::json!([]));
    assert_eq!(result["failed"].as_array().unwrap().len(), 1);
    assert!(fx.review_reasons().is_empty());
}

/// Undo is a stack, and the daemon owns it: the browser cannot undo a
/// reject, because the revert already rewrote the disk and the hunk left the
/// composed diff. Three rejects come back one at a time, most recent first,
/// and an undo over an empty stack answers empty rather than erring.
#[tokio::test]
async fn three_rejects_undo_in_reverse_order_one_at_a_time() {
    let mut fx = Fixture::new("1\n2\n3\n4\n5\n").await;
    fx.open_ledger().await;
    fx.call("call-1", "one\n2\nthree\n4\nfive\n").await;
    let _ = fx.review_reasons();

    let mut hunks = fx.list().await;
    hunks.sort_by_key(|h| h.current_range.start);
    assert_eq!(hunks.len(), 3, "{hunks:?}");
    let ids: Vec<HunkId> = hunks.iter().map(|h| h.id.clone()).collect();
    for id in &ids {
        let resp = handle_review_set_state(
            fx.request(
                "review.set_state",
                serde_json::json!({ "hunk_id": id, "state": "rejected" }),
            ),
            &fx.am,
            &fx.sm,
            &fx.event_tx,
        )
        .await;
        assert!(resp.error.is_none(), "{:?}", resp.error);
    }
    assert_eq!(fx.read(), "1\n2\n3\n4\n5\n");
    let _ = fx.review_reasons();

    let expected = [
        (ids[2].clone(), "1\n2\n3\n4\nfive\n"),
        (ids[1].clone(), "1\n2\nthree\n4\nfive\n"),
        (ids[0].clone(), "one\n2\nthree\n4\nfive\n"),
    ];
    for (id, text) in &expected {
        let resp = handle_review_undo_reject(
            fx.request("review.undo_reject", serde_json::json!({})),
            &fx.am,
            &fx.sm,
            &fx.event_tx,
        )
        .await;
        let result = resp.result.expect("an undo answers success");
        assert_eq!(result["applied"], serde_json::json!([id]));
        assert_eq!(result["failed"], serde_json::json!([]));
        assert_eq!(&fx.read(), text, "the undo restored the wrong hunk");
        assert_eq!(fx.review_reasons(), vec!["undone".to_string()]);
    }
    let restored = fx.list().await;
    assert_eq!(restored.len(), 3, "{restored:?}");
    assert!(
        restored
            .iter()
            .all(|h| h.state == ReviewState::Unreviewed && !h.reapplied),
        "an undone hunk is back in the queue as the user left it: {restored:?}"
    );

    // A fourth undo has nothing to pop.
    let resp = handle_review_undo_reject(
        fx.request("review.undo_reject", serde_json::json!({})),
        &fx.am,
        &fx.sm,
        &fx.event_tx,
    )
    .await;
    let result = resp.result.expect("an empty stack is an empty answer");
    assert_eq!(result["applied"], serde_json::json!([]));
    assert_eq!(result["failed"], serde_json::json!([]));
    assert!(fx.review_reasons().is_empty());
}

/// One user action is one batch: a bulk reject comes back in one undo, in
/// whatever order the client sent the ids.
#[tokio::test]
async fn a_bulk_reject_undoes_as_one_batch() {
    let mut fx = Fixture::new("1\n2\n3\n4\n5\n").await;
    fx.open_ledger().await;
    // Two of the three hunks change the line count, so each revert moves the
    // lines below it.
    fx.call("call-1", "one\nuno\n2\nthree\n4\nfive\ncinco\n")
        .await;
    let _ = fx.review_reasons();

    let mut hunks = fx.list().await;
    hunks.sort_by_key(|h| h.current_range.start);
    assert_eq!(hunks.len(), 3, "{hunks:?}");
    let ids: Vec<HunkId> = hunks.iter().map(|h| h.id.clone()).collect();
    // Out of file order on purpose: the reverts land at shifting lines and
    // the undo has to walk them back in the order they happened.
    let sent = vec![ids[2].clone(), ids[0].clone(), ids[1].clone()];
    let resp = handle_review_set_states(
        fx.request(
            "review.set_states",
            serde_json::json!({ "hunk_ids": sent, "state": "rejected" }),
        ),
        &fx.am,
        &fx.sm,
        &fx.event_tx,
    )
    .await;
    assert!(resp.error.is_none(), "{:?}", resp.error);
    assert_eq!(fx.read(), "1\n2\n3\n4\n5\n");
    let _ = fx.review_reasons();

    let resp = handle_review_undo_reject(
        fx.request("review.undo_reject", serde_json::json!({})),
        &fx.am,
        &fx.sm,
        &fx.event_tx,
    )
    .await;
    let result = resp.result.expect("success");
    assert_eq!(result["failed"], serde_json::json!([]));
    assert_eq!(result["applied"].as_array().unwrap().len(), 3);
    assert_eq!(fx.read(), "one\nuno\n2\nthree\n4\nfive\ncinco\n");
    assert_eq!(fx.review_reasons(), vec!["undone".to_string()]);
    assert_eq!(fx.list().await.len(), 3);
}

/// An undo writes `after_content` back over the lines the revert restored.
/// When those lines are no longer there the undo would overwrite whatever
/// is, so it is refused for that hunk — and the batch stays, because the
/// user may put the file back and ask again.
#[tokio::test]
async fn an_undo_after_the_file_moved_on_is_refused_as_stale_and_keeps_the_batch() {
    let mut fx = Fixture::new("one\n").await;
    fx.open_ledger().await;
    fx.call("call-1", "two\n").await;
    let hunk = fx.list().await.into_iter().next().expect("a hunk");
    let resp = handle_review_set_state(
        fx.request(
            "review.set_state",
            serde_json::json!({ "hunk_id": hunk.id, "state": "rejected" }),
        ),
        &fx.am,
        &fx.sm,
        &fx.event_tx,
    )
    .await;
    assert!(resp.error.is_none(), "{:?}", resp.error);
    assert_eq!(fx.read(), "one\n");
    let _ = fx.review_reasons();

    // The user edits the file the revert restored.
    std::fs::write(fx.dir.path().join("a.txt"), "ELSE\n").unwrap();
    let resp = handle_review_undo_reject(
        fx.request("review.undo_reject", serde_json::json!({})),
        &fx.am,
        &fx.sm,
        &fx.event_tx,
    )
    .await;
    let result = resp
        .result
        .expect("a refused hunk is a report, not an error");
    assert_eq!(result["applied"], serde_json::json!([]));
    let failed = result["failed"].as_array().expect("failed is a list");
    assert_eq!(failed.len(), 1, "{failed:?}");
    assert_eq!(failed[0]["hunk_id"], serde_json::json!(hunk.id));
    let reason = failed[0]["reason"].as_str().unwrap_or_default();
    assert!(reason.contains("changed since"), "{reason}");
    assert_eq!(
        fx.read(),
        "ELSE\n",
        "a stale undo overwrote the user's lines"
    );
    assert!(
        fx.review_reasons().is_empty(),
        "nothing moved, nothing to redraw"
    );

    // The user puts the file back; the batch is still there to pop.
    std::fs::write(fx.dir.path().join("a.txt"), "one\n").unwrap();
    let resp = handle_review_undo_reject(
        fx.request("review.undo_reject", serde_json::json!({})),
        &fx.am,
        &fx.sm,
        &fx.event_tx,
    )
    .await;
    let result = resp.result.expect("success");
    assert_eq!(result["applied"], serde_json::json!([hunk.id]));
    assert_eq!(fx.read(), "two\n");
}

#[tokio::test]
async fn a_comment_anchors_to_the_root_and_comes_back_with_the_hunks() {
    let mut fx = Fixture::new("one\n").await;
    fx.open_ledger().await;

    let resp = handle_review_comment(
        fx.request(
            "review.comment",
            serde_json::json!({
                "path": "a.txt",
                "line_start": 1,
                "body": "name this better",
            }),
        ),
        &fx.am,
        &fx.sm,
        &fx.event_tx,
    )
    .await;
    let comment: Comment =
        serde_json::from_value(resp.result.expect("success")["comment"].clone()).unwrap();
    assert_eq!(comment.path, "a.txt");
    assert_eq!(comment.author, CommentAuthor::Human);
    // Half-open: naming only a start line means that one line.
    assert_eq!(comment.line_range, LineRange::new(1, 2));
    assert!(!comment.resolved);
    assert_eq!(fx.review_reasons(), vec!["commented".to_string()]);

    let listed = handle_review_list_hunks(
        fx.request("review.list_hunks", serde_json::json!({})),
        &fx.am,
        &fx.sm,
    )
    .await;
    let comments = listed.result.expect("success")["comments"].clone();
    assert_eq!(comments.as_array().unwrap().len(), 1);
}

/// `line_end`, `root` and `author` are the optional half of
/// [`ReviewCommentRequest`], and a struct field that never reaches the
/// operation is invisible to every other check.
#[tokio::test]
async fn the_optional_comment_fields_reach_the_operation() {
    let fx = Fixture::new("one\n").await;
    fx.open_ledger().await;

    let resp = handle_review_comment(
        fx.request(
            "review.comment",
            serde_json::json!({
                "path": "a.txt",
                "line_start": 2,
                "line_end": 5,
                "body": "this whole block",
                "author": "agent",
            }),
        ),
        &fx.am,
        &fx.sm,
        &fx.event_tx,
    )
    .await;

    let comment: Comment =
        serde_json::from_value(resp.result.expect("success")["comment"].clone()).unwrap();
    assert_eq!(comment.line_range, LineRange::new(2, 5));
    assert_eq!(comment.author, CommentAuthor::Agent);
}

/// A caller that omits a required field is told which one. The old
/// `require_param!` named it; the request struct has to keep naming it.
#[tokio::test]
async fn a_comment_without_a_body_names_the_field_it_wants() {
    let fx = Fixture::new("one\n").await;
    fx.open_ledger().await;

    let resp = handle_review_comment(
        fx.request(
            "review.comment",
            serde_json::json!({ "path": "a.txt", "line_start": 1 }),
        ),
        &fx.am,
        &fx.sm,
        &fx.event_tx,
    )
    .await;

    let err = resp.error.expect("a comment with no body must be refused");
    assert_eq!(err.code, INVALID_PARAMS);
    assert!(err.message.contains("body"), "{}", err.message);
}

/// Same, for the two fields `review.set_state` needs beyond the session.
#[tokio::test]
async fn setting_a_state_without_a_hunk_id_names_the_field_it_wants() {
    let fx = Fixture::new("one\n").await;

    let resp = handle_review_set_state(
        fx.request(
            "review.set_state",
            serde_json::json!({ "state": "accepted" }),
        ),
        &fx.am,
        &fx.sm,
        &fx.event_tx,
    )
    .await;

    let err = resp.error.expect("a decision with no hunk must be refused");
    assert_eq!(err.code, INVALID_PARAMS);
    assert!(err.message.contains("hunk_id"), "{}", err.message);
}

#[tokio::test]
async fn commenting_without_a_ledger_is_refused() {
    let fx = Fixture::new("one\n").await;
    let resp = handle_review_comment(
        fx.request(
            "review.comment",
            serde_json::json!({ "path": "a.txt", "line_start": 1, "body": "x" }),
        ),
        &fx.am,
        &fx.sm,
        &fx.event_tx,
    )
    .await;
    assert_eq!(resp.error.expect("refused").code, INVALID_PARAMS);
}

#[tokio::test]
async fn resolving_a_comment_marks_it_and_an_unknown_id_is_refused() {
    let mut fx = Fixture::new("one\n").await;
    fx.open_ledger().await;
    let created = handle_review_comment(
        fx.request(
            "review.comment",
            serde_json::json!({ "path": "a.txt", "line_start": 1, "body": "x" }),
        ),
        &fx.am,
        &fx.sm,
        &fx.event_tx,
    )
    .await;
    let comment: Comment =
        serde_json::from_value(created.result.expect("success")["comment"].clone()).unwrap();
    let _ = fx.review_reasons();

    let resp = handle_review_resolve_comment(
        fx.request(
            "review.resolve_comment",
            serde_json::json!({ "comment_id": comment.id }),
        ),
        &fx.am,
        &fx.sm,
        &fx.event_tx,
    )
    .await;
    assert!(resp.error.is_none(), "{:?}", resp.error);
    assert!(fx.am.review.comments(&fx.session)[0].resolved);
    assert_eq!(fx.review_reasons(), vec!["comment_resolved".to_string()]);

    let unknown = handle_review_resolve_comment(
        fx.request(
            "review.resolve_comment",
            serde_json::json!({ "comment_id": "nope" }),
        ),
        &fx.am,
        &fx.sm,
        &fx.event_tx,
    )
    .await;
    assert_eq!(unknown.error.expect("refused").code, INVALID_PARAMS);
}

/// The rejection has to reach the agent as a *conversation* event, not
/// only as a UI event, or the model re-applies the same edit next turn.
/// Injection is best-effort by design: the revert already landed on disk,
/// so a session with no transcript must not turn a successful revert into
/// a failed RPC.
#[tokio::test]
async fn a_revert_succeeds_even_when_the_rejection_cannot_be_injected() {
    let fx = Fixture::new("one\n").await;
    fx.open_ledger().await;
    fx.call("call-1", "two\n").await;
    // No session registered with the SessionManager, so injection fails.
    assert!(fx.sm.get_session(&fx.session).is_none());

    let hunk = fx.list().await.into_iter().next().expect("a hunk");
    let resp = handle_review_set_state(
        fx.request(
            "review.set_state",
            serde_json::json!({ "hunk_id": hunk.id, "state": "rejected" }),
        ),
        &fx.am,
        &fx.sm,
        &fx.event_tx,
    )
    .await;
    assert!(resp.error.is_none(), "{:?}", resp.error);
    assert_eq!(fx.read(), "one\n");
}

/// The Lua/plugin review surface and the REST panel are backed by the same
/// free functions precisely so they cannot drift. They drifted here: every RPC
/// handler opens with `ensure_loaded`, and none of the five bridge methods did
/// — so a delegating agent asking a resumed session for its hunks was answered
/// `[]` with no error ("the child changed nothing") while a browser hitting the
/// same session got the queue restored from `review.jsonl`.
#[tokio::test]
async fn the_lua_bridge_restores_a_resumed_sessions_queue_like_the_handler_does() {
    use crucible_core::session::{Session, SessionType};
    use crucible_lua::DaemonSessionApi;

    let fx = Fixture::new("one\n").await;
    let _kiln = TempDir::new().unwrap();
    let session = Session::new(
        SessionType::Chat,
        vec![crate::test_support::kiln_name("kiln")],
    )
    .with_workspace(Some(fx.dir.path().to_path_buf()));
    let id = session.id.clone();
    let storage = session.storage_path(fx.sm.sessions_root());
    fx.sm.register_transient(session);

    fx.am
        .review
        .open_or_restore(&id, &storage, &[fx.dir.path().to_path_buf()])
        .await
        .unwrap();
    let handle = fx.am.review.open_bracket(&id).await.unwrap();
    std::fs::write(fx.dir.path().join("a.txt"), "two\n").unwrap();
    fx.am.review.close(&id, handle, "call-1", 1).await.unwrap();

    // A daemon restart: `review.jsonl` is on disk and nothing is in memory.
    // `register_transient` is exactly what a resume does, and it touches no
    // ledger.
    fx.am.review.clear_session(&id);
    assert!(!fx.am.review.is_open(&id));

    let bridge = crate::session_bridge::DaemonSessionBridge::new(Arc::new(RpcContext::for_test(
        Arc::new(crate::kiln_manager::KilnManager::new()),
        fx.sm.clone(),
        fx.am.clone(),
        Arc::new(crate::project_manager::ProjectManager::new(
            fx.dir.path().join("projects.json"),
        )),
        fx.event_tx.clone(),
        fx.dir.path().to_path_buf(),
    )));
    let through_lua = bridge.review_list_hunks(id.to_string()).await.unwrap();
    assert_eq!(
        through_lua.len(),
        1,
        "the plugin surface read a resumed session as having changed nothing"
    );
    assert_eq!(through_lua[0]["tool_call_ids"][0], "call-1");
}

// ── Pure helpers ───────────────────────────────────────────────────────

fn hunk(path: &str, start: u32, end: u32) -> ComposedHunk {
    ComposedHunk {
        id: HunkId::derive(
            &PhysicalRoot::from_top_level("/repo"),
            path,
            "a\n",
            "b\n",
            LineRange::new(start, end),
        ),
        root: PhysicalRoot::from_top_level("/repo"),
        path: path.to_string(),
        base_range: LineRange::new(start, end),
        current_range: LineRange::new(start, end),
        before_content: "a\n".to_string(),
        after_content: "b\n".to_string(),
        tool_call_ids: vec!["call-1".to_string()],
        state: ReviewState::Unreviewed,
        reapplied: false,
    }
}

fn base(root: &str) -> RootBase {
    RootBase {
        root: PhysicalRoot::from_top_level(root),
        base_tree: TreeSha::new("deadbeef"),
    }
}

#[test]
fn rejection_notice_names_the_file_and_the_span() {
    let notice = rejection_notice(&hunk("src/foo.rs", 88, 95));
    // 88..95 is half-open, so the last line named is 94 — the number the
    // user's editor shows, not the exclusive bound.
    assert_eq!(
        notice,
        "user rejected the edit to src/foo.rs:88-94; reverted."
    );
}

#[test]
fn rejection_notice_of_a_single_line_omits_the_range() {
    assert_eq!(hunk_location(&hunk("src/foo.rs", 12, 13)), "src/foo.rs:12");
}

/// A pure deletion leaves an empty current range. It still has to point
/// somewhere, or the agent is told an edit was rejected without being
/// told which one.
#[test]
fn rejection_notice_of_a_deletion_names_the_seam() {
    assert_eq!(hunk_location(&hunk("src/foo.rs", 40, 40)), "src/foo.rs:40");
}

#[test]
fn absolute_path_resolves_to_its_root() {
    let bases = [base("/repo")];
    let (root, rel) = resolve_root(&bases, None, Path::new("/repo/src/foo.rs")).unwrap();
    assert_eq!(*root.root, *Path::new("/repo"));
    assert_eq!(rel, "src/foo.rs");
}

/// A kiln checked out inside the workspace repo is its own root. The
/// shorter prefix also matches, and taking it would anchor the comment in
/// the wrong repository's base tree.
#[test]
fn absolute_path_prefers_the_longest_matching_root() {
    let bases = [base("/repo"), base("/repo/kiln")];
    let (root, rel) = resolve_root(&bases, None, Path::new("/repo/kiln/note.md")).unwrap();
    assert_eq!(*root.root, *Path::new("/repo/kiln"));
    assert_eq!(rel, "note.md");
}

#[test]
fn absolute_path_outside_every_root_does_not_resolve() {
    let bases = [base("/repo")];
    assert!(resolve_root(&bases, None, Path::new("/elsewhere/foo.rs")).is_err());
}

#[test]
fn relative_path_resolves_against_the_only_root() {
    let bases = [base("/repo")];
    let (root, rel) = resolve_root(&bases, None, Path::new("src/foo.rs")).unwrap();
    assert_eq!(*root.root, *Path::new("/repo"));
    assert_eq!(rel, "src/foo.rs");
}

/// Guessing here would anchor the comment against the wrong base tree and
/// silently comment on a different file that happens to share a name.
#[test]
fn relative_path_with_several_roots_is_ambiguous() {
    let bases = [base("/repo"), base("/kiln")];
    assert!(resolve_root(&bases, None, Path::new("src/foo.rs")).is_err());
}

#[test]
fn explicit_root_wins_and_accepts_an_absolute_path() {
    let bases = [base("/repo"), base("/kiln")];
    let (root, rel) =
        resolve_root(&bases, Some(Path::new("/kiln")), Path::new("/kiln/note.md")).unwrap();
    assert_eq!(*root.root, *Path::new("/kiln"));
    assert_eq!(rel, "note.md");
}

#[test]
fn explicit_root_that_the_session_does_not_track_does_not_resolve() {
    let bases = [base("/repo")];
    assert!(resolve_root(&bases, Some(Path::new("/elsewhere")), Path::new("a.rs")).is_err());
}

/// The relative arm used to be taken verbatim, so `../../etc/passwd`
/// landed on a stored `Comment` and was later handed to the
/// editor-opening path — network-reachable once the web bridge lands.
#[test]
fn a_relative_path_escaping_its_root_is_refused() {
    let dir = TempDir::new().unwrap();
    let bases = [RootBase {
        root: PhysicalRoot::from_top_level(std::fs::canonicalize(dir.path()).unwrap()),
        base_tree: TreeSha::new("deadbeef"),
    }];
    assert!(resolve_root(&bases, None, Path::new("../../etc/passwd")).is_err());
    assert!(resolve_root(&bases, None, Path::new("sub/../../escaped.txt")).is_err());
}

/// A `..` whose prefix does not exist cannot be resolved by
/// `canonicalize`, and `strip_prefix` is component-wise, so it comes back
/// out as a relative path that still escapes. The containment check is
/// what refuses it.
#[test]
fn an_unresolvable_dot_dot_is_refused_rather_than_stripped() {
    let bases = [base("/repo")];
    assert!(resolve_root(&bases, None, Path::new("/repo/../etc/passwd")).is_err());
    assert!(resolve_root(&bases, None, Path::new("../etc/passwd")).is_err());
}

/// A `..` that stays inside the root is not an escape and must still
/// resolve, or a legitimate `src/../src/foo.rs` is refused.
#[test]
fn a_dot_dot_that_stays_inside_the_root_resolves() {
    let dir = TempDir::new().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    std::fs::create_dir(root.join("src")).unwrap();
    let bases = [RootBase {
        root: PhysicalRoot::from_top_level(root.clone()),
        base_tree: TreeSha::new("deadbeef"),
    }];
    let (resolved, rel) = resolve_root(&bases, None, Path::new("src/../a.rs")).unwrap();
    assert_eq!(*resolved.root, *root);
    assert_eq!(rel, "a.rs");
}

/// Roots are stored as `git rev-parse --show-toplevel` printed them, but a
/// client names the workspace as the session registered it. Comparing the
/// two spellings raw resolves nothing.
#[test]
fn a_path_through_a_symlinked_root_resolves_to_the_tracked_root() {
    let dir = TempDir::new().unwrap();
    let real = dir.path().join("real");
    std::fs::create_dir(&real).unwrap();
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let physical = std::fs::canonicalize(&real).unwrap();

    let bases = [RootBase {
        root: PhysicalRoot::from_top_level(physical.clone()),
        base_tree: TreeSha::new("deadbeef"),
    }];
    let (root, rel) = resolve_root(&bases, None, &link.join("note.md")).unwrap();
    assert_eq!(*root.root, *physical);
    assert_eq!(rel, "note.md");

    // ...and naming the root by its symlinked spelling picks the same one.
    let (root, rel) = resolve_root(&bases, Some(&link), &link.join("note.md")).unwrap();
    assert_eq!(*root.root, *physical);
    assert_eq!(rel, "note.md");
}

#[test]
fn state_strings_are_the_wire_contract() {
    assert_eq!(
        parse_wire::<ReviewState>("unreviewed"),
        Some(ReviewState::Unreviewed)
    );
    assert_eq!(
        parse_wire::<ReviewState>("accepted"),
        Some(ReviewState::Accepted)
    );
    assert_eq!(
        parse_wire::<ReviewState>("rejected"),
        Some(ReviewState::Rejected)
    );
    assert_eq!(parse_wire::<ReviewState>("Accepted"), None);
    assert_eq!(parse_wire::<ReviewState>(""), None);
}

#[test]
fn author_strings_are_the_wire_contract() {
    assert_eq!(
        parse_wire::<CommentAuthor>("human"),
        Some(CommentAuthor::Human)
    );
    assert_eq!(
        parse_wire::<CommentAuthor>("agent"),
        Some(CommentAuthor::Agent)
    );
    assert_eq!(parse_wire::<CommentAuthor>("bot"), None);
}

/// A stale client acting on a hunk that moved must be told to re-list,
/// not that the daemon is broken.
#[test]
fn caller_recoverable_errors_map_to_invalid_params() {
    for err in [
        ReviewError::NoLedger("s".into()),
        ReviewError::NoTrackableRoots("s".into()),
        ReviewError::UnknownHunk(HunkId::from("h".to_string())),
        ReviewError::Stale {
            path: "a.rs".into(),
        },
        ReviewError::ExternalHunk(HunkId::from("h".to_string())),
        ReviewError::UnknownComment("c".into()),
        ReviewError::NotAGitRepo {
            path: PathBuf::from("/x"),
        },
    ] {
        let resp = review_error_to_response(None, err);
        assert_eq!(resp.error.expect("error").code, INVALID_PARAMS);
    }
}

#[test]
fn git_and_io_failures_map_to_internal_error() {
    for err in [
        ReviewError::Git("write-tree failed".into()),
        ReviewError::Io(std::io::Error::other("disk")),
    ] {
        let resp = review_error_to_response(None, err);
        assert_eq!(resp.error.expect("error").code, INTERNAL_ERROR);
    }
}
