use super::*;
use crate::test_support::temp_session_manager;
use crucible_core::protocol::rpc::INTERNAL_ERROR;
use crucible_core::protocol::RequestId;
use crucible_core::session::{PhysicalRoot, SnapshotId};
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
            review_snapshot_root: crate::test_support::scratch_snapshot_root(),
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

    /// Start a turn on the session's scheduler-owned tree the way a send
    /// does: one `User` node, then the `Agent` node the tool calls run under.
    /// Answers the node id a bracket closed in this turn would record.
    async fn begin_turn(&self, text: &str) -> u32 {
        use crucible_core::turn::NodeContent;
        let tree = self
            .am
            .get_or_rebuild_session_tree(&self.session, std::path::Path::new("/nonexistent.jsonl"))
            .await;
        let mut tree = tree.lock().await;
        let cursor = tree.current();
        let user = tree.add_child_and_advance(
            cursor,
            NodeContent::User {
                text: text.to_string(),
            },
        );
        tree.add_child_and_advance(
            user,
            NodeContent::Agent {
                text: String::new(),
            },
        );
        tree.current().index()
    }

    /// One bracketed "tool call" that rewrites `file`, closed at `node_id` —
    /// the turn coordinate the interval keeps.
    async fn call_at(&self, tool_call_id: &str, file: &str, contents: &str, node_id: u32) {
        let handle = self.am.review.open_bracket(&self.session).await.unwrap();
        std::fs::write(self.dir.path().join(file), contents).unwrap();
        self.am
            .review
            .close(&self.session, handle, tool_call_id, node_id)
            .await
            .unwrap();
    }

    /// The listing under one scope, as the handler answers it.
    async fn list_scoped(&self, scope: &str) -> serde_json::Value {
        let resp = handle_review_list_hunks(
            self.request("review.list_hunks", serde_json::json!({ "scope": scope })),
            &self.am,
            &self.sm,
        )
        .await;
        resp.result.expect("hunks")
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

/// The turn scope is a filter over the composed diff, decided by the daemon
/// from the turn coordinate every interval carries: a hunk is the current
/// turn's when one of its calls closed at or after the turn's first node.
/// An external hunk has no call and is never the turn's.
#[tokio::test]
async fn turn_scope_lists_only_hunks_the_current_turn_touched() {
    let fx = Fixture::new("1\n2\n3\n4\n5\n6\n7\n8\n9\n").await;
    std::fs::write(fx.dir.path().join("b.txt"), "alpha\n").unwrap();
    git(fx.dir.path(), &["add", "."]).await;
    git(fx.dir.path(), &["commit", "-q", "-m", "second file"]).await;
    fx.open_ledger().await;

    let first = fx.begin_turn("edit a").await;
    fx.call_at("call-1", "a.txt", "one\n2\n3\n4\n5\n6\n7\n8\n9\n", first)
        .await;
    let second = fx.begin_turn("edit b").await;
    fx.call_at("call-2", "b.txt", "ALPHA\n", second).await;
    // The user's own edit, seen by no bracket, far enough from the first
    // hunk to compose as its own.
    std::fs::write(
        fx.dir.path().join("a.txt"),
        "one\n2\n3\n4\n5\n6\n7\n8\nnine\n",
    )
    .unwrap();

    let session = fx.list_scoped("session").await;
    let paths = |v: &serde_json::Value| -> Vec<String> {
        let mut out: Vec<String> = v["hunks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| h["path"].as_str().unwrap().to_string())
            .collect();
        out.sort();
        out
    };
    assert_eq!(
        paths(&session),
        vec!["a.txt", "a.txt", "b.txt"],
        "the session scope is the whole composed diff: {session}"
    );
    assert_eq!(session["scope"], serde_json::json!("session"));

    let turn = fx.list_scoped("turn").await;
    assert_eq!(
        paths(&turn),
        vec!["b.txt"],
        "the turn scope keeps only what the current turn's calls wrote: {turn}"
    );
    assert_eq!(
        turn["hunks"][0]["tool_call_ids"],
        serde_json::json!(["call-2"])
    );
    assert_eq!(turn["scope"], serde_json::json!("turn"));

    // Absent means session, so every client written before scopes still
    // reads the whole diff.
    let unscoped = fx.list().await;
    assert_eq!(unscoped.len(), 3);
}

/// A scope the daemon does not know is refused before the ledger is read,
/// like an unknown state.
#[tokio::test]
async fn an_unknown_scope_is_refused() {
    let fx = Fixture::new("one\n").await;
    fx.open_ledger().await;
    let resp = handle_review_list_hunks(
        fx.request(
            "review.list_hunks",
            serde_json::json!({ "scope": "workspace" }),
        ),
        &fx.am,
        &fx.sm,
    )
    .await;
    let err = resp.error.expect("refused");
    assert_eq!(err.code, INVALID_PARAMS);
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
    assert!(fx.am.review.comments(&fx.session).unwrap()[0].resolved);
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

/// `review.comment` and `review.resolve_comment` are aliases of the
/// `diff.*` methods on the session record: a comment made through one name
/// is the comment that the other name lists and resolves.
#[tokio::test]
async fn review_comment_is_an_alias_for_the_session_record() {
    use crate::diff::comments::ListedComment;
    use crate::kiln_manager::KilnManager;
    use crate::project_manager::ProjectManager;
    use crate::rpc_client::{DiffCommentsReply, DiffCommentsRequest, DiffResolveCommentRequest};
    use crate::server::diff::Admission;
    use crate::server::diff_comments::{handle_diff_comments, handle_diff_resolve_comment};
    use crucible_core::diff::{DiffsetId, DiffsetSource};
    use crucible_core::session::{CommentAnchor, CommentSide, SessionId};

    let mut fx = Fixture::new("one\ntwo\n").await;
    fx.open_ledger().await;
    let created = handle_review_comment(
        fx.request(
            "review.comment",
            serde_json::json!({ "path": "a.txt", "line_start": 2, "body": "x" }),
        ),
        &fx.am,
        &fx.sm,
        &fx.event_tx,
    )
    .await;
    let comment: Comment =
        serde_json::from_value(created.result.expect("success")["comment"].clone()).unwrap();
    let session = SessionId::parse(&fx.session).unwrap();
    let base = fx.am.review.ledger(&fx.session).unwrap().session_base()[0].clone();
    assert_eq!(comment.diffset, DiffsetId::for_session(&session));
    assert_eq!(comment.anchor, CommentAnchor::Snapshot(base.base_tree));
    assert_eq!(comment.side, CommentSide::Current);
    assert_eq!(comment.quoted, "two\n");
    assert_eq!(fx.review_reasons(), vec!["commented".to_string()]);

    // `diff.comments` on the session record lists the alias comment.
    let store = TempDir::new().unwrap();
    let projects = Arc::new(ProjectManager::new(store.path().join("projects.json")));
    let kilns = Arc::new(KilnManager::new());
    let admission = || Admission {
        projects: &projects,
        kilns: &kilns,
        sessions: &fx.sm,
        review: &fx.am.review,
        proposals: fx.am.proposals(),
    };
    let source = DiffsetSource::SessionRecord { session };
    let listed = handle_diff_comments(
        fx.request(
            "diff.comments",
            serde_json::to_value(DiffCommentsRequest {
                source: source.clone(),
            })
            .unwrap(),
        ),
        admission(),
    )
    .await;
    let listed: DiffCommentsReply = serde_json::from_value(listed.result.expect("listed")).unwrap();
    assert_eq!(
        listed.comments,
        vec![ListedComment {
            comment: comment.clone(),
            outdated: false,
        }]
    );

    // `diff.resolve_comment` resolves it, and `review.list_hunks` sees that.
    let resolved = handle_diff_resolve_comment(
        fx.request(
            "diff.resolve_comment",
            serde_json::to_value(DiffResolveCommentRequest {
                source,
                comment_id: comment.id.clone(),
            })
            .unwrap(),
        ),
        admission(),
        &fx.event_tx,
    )
    .await;
    assert!(resolved.error.is_none(), "{:?}", resolved.error);
    assert_eq!(fx.review_reasons(), vec!["comment_resolved".to_string()]);
    let hunks = handle_review_list_hunks(
        fx.request("review.list_hunks", serde_json::json!({})),
        &fx.am,
        &fx.sm,
    )
    .await;
    assert_eq!(
        hunks.result.expect("success")["comments"][0]["resolved"],
        serde_json::json!(true)
    );
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

fn base(root: &str) -> RootBase {
    RootBase {
        root: PhysicalRoot::from_top_level(root),
        base_tree: SnapshotId::git("deadbeef"),
    }
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
        base_tree: SnapshotId::git("deadbeef"),
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
        base_tree: SnapshotId::git("deadbeef"),
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
        base_tree: SnapshotId::git("deadbeef"),
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

/// A caller that asked for something the ledger cannot answer must be told
/// so, not that the daemon is broken.
#[test]
fn caller_recoverable_errors_map_to_invalid_params() {
    for err in [
        ReviewError::NoLedger("s".into()),
        ReviewError::NoTrackableRoots("s".into()),
        ReviewError::UnknownComment("c".into()),
        ReviewError::InvalidSession("bad id".into()),
        ReviewError::InvalidComment("bad range".into()),
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

// ── The crossing: a plugin session's own writes are its own review queue ────

/// An agent whose one turn writes a note with `create_note`, then waits for
/// the tool result before it ends the turn.
///
/// Modelled on `session_bridge/tests/mod.rs`'s `BashCallingAgent`: the agent
/// only *asks* for the call. The real scheduler, the real review bracket and
/// the real note tool do the work, which is the point — a double at any of
/// those three would prove nothing about the crossing.
struct NoteWritingAgent;

#[async_trait::async_trait]
impl crucible_core::turn::Agent for NoteWritingAgent {
    fn capabilities(&self) -> crucible_core::turn::AgentCapabilities {
        crucible_core::turn::AgentCapabilities::default()
    }
    async fn turn<'a>(
        &'a mut self,
        ctx: crucible_core::turn::TurnContext,
    ) -> Result<
        futures::stream::BoxStream<'a, crucible_core::turn::TurnEvent>,
        crucible_core::turn::AgentError,
    > {
        use crucible_core::turn::{StopReason, TurnEvent};
        let mut inbound = ctx.inbound;
        let body = async_stream::stream! {
            yield TurnEvent::ToolCall {
                id: "call-1".to_string(),
                name: "create_note".to_string(),
                args: serde_json::json!({
                    "path": "Socket rules.md",
                    "content": NOTE_TEXT,
                }),
                diffs: Vec::new(),
            };
            yield TurnEvent::ToolBatchEnd;
            if let Some(rx) = inbound.as_mut() {
                while let Some(event) = rx.recv().await {
                    if matches!(event, TurnEvent::ToolResult { .. }) {
                        break;
                    }
                }
            }
            yield TurnEvent::Done { stop_reason: StopReason::EndTurn };
        };
        Ok(Box::pin(body))
    }
    async fn cancel(&self) -> Result<(), crucible_core::turn::AgentError> {
        Ok(())
    }
    async fn switch_model(&mut self, _: &str) -> Result<(), crucible_core::turn::NotSupported> {
        Err(crucible_core::turn::NotSupported::new("switch_model"))
    }
}

crucible_core::impl_unsupported_session_knobs!(NoteWritingAgent);

#[async_trait::async_trait]
impl crucible_core::traits::chat::AgentHandle for NoteWritingAgent {
    async fn send_message_fire_and_forget(
        &mut self,
        _: String,
    ) -> crucible_core::traits::chat::ChatResult<()> {
        Ok(())
    }
    async fn clear_history(&mut self) -> crucible_core::traits::chat::ChatResult<()> {
        Ok(())
    }
    async fn set_mode_str(&mut self, _: &str) -> crucible_core::traits::chat::ChatResult<()> {
        Ok(())
    }
    fn get_mode_id(&self) -> &str {
        "auto"
    }
}

/// The note body the agent writes, and the `after_content` the review must
/// answer with.
const NOTE_TEXT: &str = "# Socket rules\n\nThe daemon socket is per-uid.\n";

/// The reflection pass stopped staging files: it writes its notes with the
/// note tools, in its own `plugin` session, in `auto` mode. This is the
/// crossing that has to hold for that to mean anything — a note written by a
/// plugin session's turn is a hunk in *that session's* review ledger, with
/// the note's path and the note's text, so a human accepts or rejects it in
/// the Changes panel.
///
/// The kiln is a plain directory with no `.git`, which is the shape a kiln
/// usually has, and the one `RootBackend::Plain` tracks.
///
/// The permissions config allows the call. The stance an unattended pass
/// actually runs under comes from `runtime/defaults/init.luau`, which needs a
/// plugin VM; what this test is about is the ledger, and a prompt nobody can
/// answer would only hang it.
#[tokio::test]
async fn a_plugin_sessions_note_write_lands_in_its_own_review_ledger() {
    use crate::agent_manager::AgentManagerParams;
    use crate::background_manager::BackgroundJobManager;
    use crate::kiln_manager::KilnManager;
    use crucible_core::config::components::permissions::{PermissionConfig, PermissionMode};
    use crucible_core::session::{SessionAgent, SessionType};

    let kiln = TempDir::new().unwrap();
    assert!(
        !kiln.path().join(".git").exists(),
        "the fixture kiln must be outside git"
    );
    let snapshots = TempDir::new().unwrap();

    let (event_tx, _events) = broadcast::channel(256);
    let sm = crate::test_support::temp_session_manager_with_kilns(&[("notes", kiln.path())]);
    let am = Arc::new(AgentManager::new(AgentManagerParams {
        kiln_manager: Arc::new(KilnManager::new()),
        session_manager: sm.clone(),
        background_manager: Arc::new(BackgroundJobManager::new(event_tx.clone())),
        mcp_gateway: None,
        llm_config: None,
        acp_config: None,
        context_config: None,
        permission_config: Some(PermissionConfig {
            default: PermissionMode::Allow,
            ..Default::default()
        }),
        plugin_loader: None,
        card_roots: Default::default(),
        // A subdirectory, so the comment store beside it is in this
        // fixture and not in the shared temporary directory.
        review_snapshot_root: snapshots.path().join("review-snapshots"),
    }));
    am.set_agent_factory_override(Box::new(|_, _| {
        Box::pin(async {
            Ok(Box::new(NoteWritingAgent)
                as Box<
                    dyn crucible_core::traits::chat::AgentHandle + Send + Sync,
                >)
        })
    }));

    // A pass has no workspace: the kiln is the only root it writes to, so it
    // is the only root the ledger tracks.
    let session = sm
        .create_session(
            SessionType::Plugin,
            vec![crate::test_support::kiln_name("notes")],
            None,
            None,
        )
        .await
        .unwrap();
    let session_id = session.id.to_string();
    am.configure_agent(
        &session_id,
        SessionAgent {
            mode: None,
            agent_type: "internal".to_string(),
            agent_name: None,
            provider_key: Some("ollama".to_string()),
            provider: crucible_core::config::BackendType::Ollama,
            model: "llama3.2".to_string(),
            system_prompt: "You are a reflection reviewer.".to_string(),
            max_context_tokens: None,
            endpoint: None,
            env_overrides: Default::default(),
            mcp_servers: Vec::new(),
            agent_card_name: None,
            agent_description: None,
            delegation_config: None,
            precognition_enabled: false,
            context_budget: None,
            context_strategy: Default::default(),
            tool_policy: None,
        },
    )
    .await
    .unwrap();
    // What `aux:set_mode("auto")` does: the gate does not hold an `auto` turn,
    // so the write is bracketed and queued rather than parked at the gate.
    am.set_mode(&session_id, "auto", None).await.unwrap();

    let (_message_id, done) = am
        .send_message_notified(&session_id, "reflect".to_string(), &event_tx, false, None)
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(60), done)
        .await
        .expect("the turn finishes")
        .expect("the turn reports an outcome");

    assert_eq!(
        std::fs::read_to_string(kiln.path().join("Socket rules.md")).unwrap(),
        NOTE_TEXT,
        "the note has to be on disk before the review can dispose of it"
    );

    let resp = handle_review_list_hunks(
        Request {
            jsonrpc: "2.0".to_string(),
            id: Some(RequestId::Number(1)),
            method: "review.list_hunks".to_string(),
            params: serde_json::json!({ "session_id": session_id }),
        },
        &am,
        &sm,
    )
    .await;
    let result = resp.result.expect("the pass's own queue");
    let hunks: Vec<ComposedHunk> = serde_json::from_value(result["hunks"].clone()).unwrap();

    assert_eq!(hunks.len(), 1, "one note written, one hunk: {hunks:?}");
    assert_eq!(hunks[0].path, "Socket rules.md");
    assert_eq!(hunks[0].after_content, NOTE_TEXT);
    assert!(
        hunks[0].before_content.is_empty(),
        "a new note has no before text: {:?}",
        hunks[0].before_content
    );
    assert!(
        hunks[0].tool_call_ids.iter().any(|id| id == "call-1"),
        "the hunk is attributed to the note call: {:?}",
        hunks[0].tool_call_ids
    );
}
