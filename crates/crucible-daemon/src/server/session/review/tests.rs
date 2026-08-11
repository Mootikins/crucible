use super::*;
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
        use crate::tools::workspace::WorkspaceTools;

        let dir = TempDir::new().unwrap();
        git(dir.path(), &["init", "-q"]).await;
        git(dir.path(), &["config", "user.email", "t@t"]).await;
        git(dir.path(), &["config", "user.name", "t"]).await;
        std::fs::write(dir.path().join("a.txt"), initial).unwrap();
        git(dir.path(), &["add", "."]).await;
        git(dir.path(), &["commit", "-q", "-m", "init"]).await;

        let (event_tx, events) = broadcast::channel(64);
        let kiln_manager = Arc::new(KilnManager::new());
        let session_manager = Arc::new(SessionManager::new());
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
            workspace_tools: Arc::new(WorkspaceTools::new(dir.path().to_path_buf())),
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
    let kiln = TempDir::new().unwrap();
    let session = Session::new(SessionType::Chat, kiln.path().to_path_buf())
        .with_workspace(fx.dir.path().to_path_buf());
    let id = session.id.clone();
    let storage = session.storage_path();
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

    let bridge = crate::session_bridge::DaemonSessionBridge::new(
        fx.sm.clone(),
        fx.am.clone(),
        fx.event_tx.clone(),
    );
    let through_lua = bridge.review_list_hunks(id.clone()).await.unwrap();
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
    assert_eq!(parse_state("unreviewed"), Some(ReviewState::Unreviewed));
    assert_eq!(parse_state("accepted"), Some(ReviewState::Accepted));
    assert_eq!(parse_state("rejected"), Some(ReviewState::Rejected));
    assert_eq!(parse_state("Accepted"), None);
    assert_eq!(parse_state(""), None);
}

#[test]
fn author_strings_are_the_wire_contract() {
    assert_eq!(parse_author("human"), Some(CommentAuthor::Human));
    assert_eq!(parse_author("agent"), Some(CommentAuthor::Agent));
    assert_eq!(parse_author("bot"), None);
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
