use super::*;
use crate::kiln_manager::KilnManager;
use crate::project_manager::ProjectManager;
use crate::protocol::SessionEventMessage;
use crate::protocol::{RequestId, RpcError, INVALID_PARAMS};
use crate::session_manager::SessionManager;
use crate::test_support::{git, init_repo};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::broadcast;

/// The daemon state of one test, as in the `server::diff` tests, and the
/// event channel that the comment RPCs announce on.
struct Daemon {
    projects: Arc<ProjectManager>,
    kilns: Arc<KilnManager>,
    sessions: Arc<SessionManager>,
    review: Arc<ReviewLedgers>,
    proposals: crate::proposals::ProposalStore,
    event_tx: crate::EventBus,
    events: broadcast::Receiver<SessionEventMessage>,
    _store: TempDir,
}

impl Daemon {
    fn new() -> Self {
        let store = TempDir::new().unwrap();
        let (event_tx, events) = crate::EventBus::channel(16);
        Self {
            projects: Arc::new(ProjectManager::new(store.path().join("projects.json"))),
            kilns: Arc::new(KilnManager::new()),
            sessions: Arc::new(SessionManager::with_storage(
                crate::test_support::temp_session_storage(),
            )),
            review: Arc::new(ReviewLedgers::for_tests(store.path().join("snapshots"))),
            proposals: crate::proposals::ProposalStore::new(store.path().join("proposals")),
            event_tx,
            events,
            _store: store,
        }
    }

    fn admission(&self) -> Admission<'_> {
        Admission {
            projects: &self.projects,
            kilns: &self.kilns,
            sessions: &self.sessions,
            review: &self.review,
            proposals: &self.proposals,
        }
    }

    async fn call<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: impl serde::Serialize,
    ) -> Result<T, RpcError> {
        let req = Request {
            jsonrpc: "2.0".to_string(),
            id: Some(RequestId::Number(1)),
            method: method.to_string(),
            params: serde_json::to_value(params).unwrap(),
        };
        let resp = match method {
            "diff.comment" => handle_diff_comment(req, self.admission(), &self.event_tx).await,
            "diff.resolve_comment" => {
                handle_diff_resolve_comment(req, self.admission(), &self.event_tx).await
            }
            "diff.delete_comment" => {
                handle_diff_delete_comment(req, self.admission(), &self.event_tx).await
            }
            "diff.comments" => handle_diff_comments(req, self.admission()).await,
            other => panic!("no handler for {other}"),
        };
        match resp.error {
            Some(error) => Err(error),
            None => Ok(serde_json::from_value(resp.result.expect("a result")).unwrap()),
        }
    }

    async fn comment(&self, request: DiffCommentRequest) -> Result<DiffCommentReply, RpcError> {
        self.call("diff.comment", request).await
    }

    async fn comments(&self, source: &DiffsetSource) -> Result<DiffCommentsReply, RpcError> {
        self.call(
            "diff.comments",
            DiffCommentsRequest {
                source: source.clone(),
            },
        )
        .await
    }

    async fn resolve(
        &self,
        source: &DiffsetSource,
        comment_id: &str,
    ) -> Result<DiffResolveCommentReply, RpcError> {
        self.call(
            "diff.resolve_comment",
            DiffResolveCommentRequest {
                source: source.clone(),
                comment_id: comment_id.to_string(),
            },
        )
        .await
    }

    async fn delete(
        &self,
        source: &DiffsetSource,
        comment_id: &str,
    ) -> Result<DiffDeleteCommentReply, RpcError> {
        self.call(
            "diff.delete_comment",
            DiffDeleteCommentRequest {
                source: source.clone(),
                comment_id: comment_id.to_string(),
            },
        )
        .await
    }

    /// The reasons of the `review_changed` events since the last call.
    fn review_reasons(&mut self) -> Vec<String> {
        let mut reasons = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            if event.event == "review_changed" {
                reasons.push(event.data["reason"].as_str().unwrap_or_default().into());
            }
        }
        reasons
    }
}

/// A registered repository on `main`, with a checked-out branch `feature`.
async fn repo(daemon: &Daemon, files: &[(&str, &str)]) -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path(), files).await;
    git(tmp.path(), &["branch", "-M", "main"]).await;
    git(tmp.path(), &["checkout", "-q", "-b", "feature"]).await;
    let root = daemon.projects.register(tmp.path()).unwrap().path;
    (tmp, root)
}

/// The branch source of `root` with the default base and the working tree.
fn working_tree(root: &Path) -> DiffsetSource {
    DiffsetSource::Branch {
        root: PhysicalRoot::from_top_level(root),
        base: String::new(),
        head: None,
    }
}

/// A human comment on `path`, lines `start..end`, of the diffset of `source`.
fn request(
    source: &DiffsetSource,
    path: &str,
    side: CommentSide,
    start: u32,
    end: u32,
) -> DiffCommentRequest {
    DiffCommentRequest {
        source: source.clone(),
        root: None,
        path: path.into(),
        from: None,
        side,
        line_start: start,
        line_end: Some(end),
        body: format!("about {path}"),
        author: None,
    }
}

fn record(session: &str) -> DiffsetSource {
    DiffsetSource::SessionRecord {
        session: SessionId::parse(session).unwrap(),
    }
}

#[tokio::test]
async fn a_comment_anchors_to_its_diffset() {
    let mut daemon = Daemon::new();
    let (tmp, root) = repo(&daemon, &[("a.md", "one\ntwo\n"), ("old.md", "x\ny\n")]).await;
    let dir = tmp.path();
    git(dir, &["mv", "old.md", "new.md"]).await;
    fs::write(dir.join("a.md"), "one\nTWO\nthree\n").unwrap();
    let merge_base = git(dir, &["rev-parse", "main"]).await.trim().to_string();

    // A branch comment belongs to the resolved branch source: the daemon
    // names the default branch, so the id is the id of `main`.
    let source = working_tree(&root);
    let resolved = DiffsetSource::Branch {
        root: PhysicalRoot::from_top_level(&root),
        base: "main".into(),
        head: None,
    };
    let reply = daemon
        .comment(request(&source, "a.md", CommentSide::Current, 2, 4))
        .await
        .unwrap();
    let comment = &reply.comment;
    assert_eq!(reply.diffset, resolved.id());
    assert_eq!(comment.diffset, resolved.id());
    assert_eq!(comment.anchor, CommentAnchor::Commit(merge_base));
    assert_eq!(comment.root, PhysicalRoot::from_top_level(&root));
    assert_eq!(comment.quoted, "TWO\nthree\n");
    assert_eq!(comment.author, CommentAuthor::Human);
    assert_eq!(
        daemon.review.comment_store().list(&resolved.id()).unwrap(),
        vec![comment.clone()]
    );
    assert!(
        daemon.review_reasons().is_empty(),
        "a branch comment has no session to tell"
    );

    // A base-side comment of a renamed file quotes the old path.
    let mut renamed = request(&source, "new.md", CommentSide::Base, 2, 3);
    renamed.from = Some("old.md".into());
    let reply = daemon.comment(renamed).await.unwrap();
    assert_eq!(reply.comment.quoted, "y\n");
    assert_eq!(reply.comment.path, "new.md");

    // A session record comment belongs to the record of its session and is
    // anchored in the session base snapshot.
    let session = TempDir::new().unwrap();
    init_repo(session.path(), &[("b.md", "base\n")]).await;
    daemon
        .review
        .open("chat-1", &[session.path().to_path_buf()])
        .await
        .unwrap();
    fs::write(session.path().join("b.md"), "base\nnew\n").unwrap();
    let base = daemon.review.ledger("chat-1").unwrap().session_base()[0].clone();
    let mut on_record = request(&record("chat-1"), "b.md", CommentSide::Base, 1, 2);
    on_record.root = Some(base.root.clone());
    on_record.author = Some(CommentAuthor::Agent);
    let reply = daemon.comment(on_record.clone()).await.unwrap();
    assert_eq!(reply.diffset, record("chat-1").id());
    assert_eq!(
        reply.comment.anchor,
        CommentAnchor::Snapshot(base.base_tree)
    );
    assert_eq!(reply.comment.quoted, "base\n");
    assert_eq!(reply.comment.author, CommentAuthor::Agent);
    assert_eq!(daemon.review_reasons(), vec!["commented".to_string()]);

    // A session record file needs its root.
    on_record.root = None;
    let error = daemon.comment(on_record).await.unwrap_err();
    assert_eq!(error.code, INVALID_PARAMS);
}

#[tokio::test]
async fn a_comment_that_cannot_be_anchored_is_refused() {
    let daemon = Daemon::new();
    let (_tmp, root) = repo(&daemon, &[("a.md", "one\n")]).await;
    let source = working_tree(&root);

    for (start, end) in [(0, 1), (2, 2), (3, 2)] {
        let error = daemon
            .comment(request(&source, "a.md", CommentSide::Current, start, end))
            .await
            .unwrap_err();
        assert_eq!(error.code, INVALID_PARAMS, "{start}..{end}");
    }
    for path in ["../outside", "/etc/passwd", ""] {
        let error = daemon
            .comment(request(&source, path, CommentSide::Current, 1, 2))
            .await
            .unwrap_err();
        assert_eq!(error.code, INVALID_PARAMS, "{path}");
    }
    // A root that no admission names.
    let stranger = TempDir::new().unwrap();
    init_repo(stranger.path(), &[("a.md", "one\n")]).await;
    let error = daemon
        .comment(request(
            &working_tree(stranger.path()),
            "a.md",
            CommentSide::Current,
            1,
            2,
        ))
        .await
        .unwrap_err();
    assert_eq!(error.code, INVALID_PARAMS);
    // An unknown proposal has no diffset.
    let proposal = DiffsetSource::Proposal {
        id: "6f1c1d2e-3b4a-4c5d-8e9f-0a1b2c3d4e5f".parse().unwrap(),
    };
    let error = daemon
        .comment(request(&proposal, "a.md", CommentSide::Current, 1, 2))
        .await
        .unwrap_err();
    assert_eq!(error.code, INVALID_PARAMS);
    assert!(error.message.contains("no proposal"), "{error:?}");
    // No refusal stored a comment.
    let listed = daemon.comments(&source).await.unwrap();
    assert!(listed.comments.is_empty(), "{listed:?}");
}

#[tokio::test]
async fn resolving_an_unknown_comment_is_an_error() {
    let mut daemon = Daemon::new();
    let (_tmp, root) = repo(&daemon, &[("a.md", "one\n")]).await;
    let source = working_tree(&root);
    let comment = daemon
        .comment(request(&source, "a.md", CommentSide::Current, 1, 2))
        .await
        .unwrap()
        .comment;

    let error = daemon.resolve(&source, "nope").await.unwrap_err();
    assert_eq!(error.code, INVALID_PARAMS);
    assert_eq!(error.message, "unknown comment nope");

    // The id of a comment of another diffset is unknown here.
    let error = daemon
        .resolve(&record("chat-1"), &comment.id)
        .await
        .unwrap_err();
    assert_eq!(error.message, format!("unknown comment {}", comment.id));
    assert!(daemon.review_reasons().is_empty());

    let reply = daemon.resolve(&source, &comment.id).await.unwrap();
    assert_eq!(reply.diffset, comment.diffset);
    assert!(reply.resolved);
    let listed = daemon.comments(&source).await.unwrap();
    assert!(listed.comments[0].comment.resolved);
}

/// Delete removes the comment. Resolve keeps it; the two are not the same
/// operation, and a deleted comment never comes back.
#[tokio::test]
async fn deleting_a_comment_removes_it_from_the_store() {
    let mut daemon = Daemon::new();
    let (_tmp, root) = repo(&daemon, &[("a.md", "one\ntwo\n")]).await;
    let source = working_tree(&root);
    let first = daemon
        .comment(request(&source, "a.md", CommentSide::Current, 1, 2))
        .await
        .unwrap()
        .comment;
    let second = daemon
        .comment(request(&source, "a.md", CommentSide::Current, 2, 3))
        .await
        .unwrap()
        .comment;

    // An unknown id, and the id of a comment of another diffset, are errors.
    let error = daemon.delete(&source, "nope").await.unwrap_err();
    assert_eq!(error.code, INVALID_PARAMS);
    assert_eq!(error.message, "unknown comment nope");
    let error = daemon
        .delete(&record("chat-1"), &first.id)
        .await
        .unwrap_err();
    assert_eq!(error.message, format!("unknown comment {}", first.id));

    let reply = daemon.delete(&source, &first.id).await.unwrap();
    assert_eq!(reply.diffset, first.diffset);
    assert!(reply.deleted);
    // The listing holds only the comment that stays, resolved or not.
    let listed = daemon.comments(&source).await.unwrap();
    let ids: Vec<_> = listed
        .comments
        .iter()
        .map(|l| l.comment.id.as_str())
        .collect();
    assert_eq!(ids, vec![second.id.as_str()]);
    // A second delete finds nothing, and resolve cannot bring the comment back.
    assert_eq!(
        daemon.delete(&source, &first.id).await.unwrap_err().message,
        format!("unknown comment {}", first.id)
    );
    assert_eq!(
        daemon
            .resolve(&source, &first.id)
            .await
            .unwrap_err()
            .message,
        format!("unknown comment {}", first.id)
    );
    assert!(
        daemon.review_reasons().is_empty(),
        "a branch comment has no session to tell"
    );
}

/// A session record tells its clients that a comment went away, so the
/// Changes panel and the diff pane agree.
#[tokio::test]
async fn deleting_a_record_comment_tells_the_session() {
    let mut daemon = Daemon::new();
    let session = TempDir::new().unwrap();
    init_repo(session.path(), &[("b.md", "base\n")]).await;
    daemon
        .review
        .open("chat-1", &[session.path().to_path_buf()])
        .await
        .unwrap();
    let base = daemon.review.ledger("chat-1").unwrap().session_base()[0].clone();
    let source = record("chat-1");
    let mut on_record = request(&source, "b.md", CommentSide::Current, 1, 2);
    on_record.root = Some(base.root.clone());
    let comment = daemon.comment(on_record).await.unwrap().comment;
    assert_eq!(daemon.review_reasons(), vec!["commented"]);

    daemon.delete(&source, &comment.id).await.unwrap();
    assert_eq!(daemon.review_reasons(), vec!["comment_deleted"]);
    assert!(daemon.comments(&source).await.unwrap().comments.is_empty());
}

#[tokio::test]
async fn diff_comments_projects_each_range() {
    let daemon = Daemon::new();
    let (tmp, root) = repo(
        &daemon,
        &[("a.md", "one\ntwo\nthree\n"), ("old.md", "x\ny\n")],
    )
    .await;
    let dir = tmp.path();
    git(dir, &["mv", "old.md", "new.md"]).await;
    fs::write(dir.join("a.md"), "one\ntwo\nthree\n").unwrap();
    let source = working_tree(&root);

    let kept = daemon
        .comment(request(&source, "a.md", CommentSide::Base, 1, 2))
        .await
        .unwrap()
        .comment;
    let moved = daemon
        .comment(request(&source, "a.md", CommentSide::Current, 2, 3))
        .await
        .unwrap()
        .comment;
    let gone = daemon
        .comment(request(&source, "a.md", CommentSide::Current, 3, 4))
        .await
        .unwrap()
        .comment;
    let mut on_old = request(&source, "new.md", CommentSide::Base, 2, 3);
    on_old.from = Some("old.md".into());
    let renamed = daemon.comment(on_old).await.unwrap().comment;

    // Two lines above `two`, and `three` is gone.
    fs::write(dir.join("a.md"), "zero\nhalf\none\ntwo\n").unwrap();

    let listed = daemon.comments(&source).await.unwrap();
    assert_eq!(listed.diffset, kept.diffset);
    let rows: Vec<_> = listed
        .comments
        .iter()
        .map(|l| (l.comment.id.as_str(), l.comment.line_range, l.outdated))
        .collect();
    assert_eq!(
        rows,
        vec![
            // The base side did not change.
            (kept.id.as_str(), LineRange::new(1, 2), false),
            (moved.id.as_str(), LineRange::new(4, 5), false),
            (gone.id.as_str(), LineRange::new(3, 4), true),
            // The base text of a renamed file is at its old path.
            (renamed.id.as_str(), LineRange::new(2, 3), false),
        ]
    );
}

#[tokio::test]
async fn a_session_record_lists_its_comments_with_or_without_a_ledger() {
    let daemon = Daemon::new();
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path(), &[("a.md", "one\ntwo\n")]).await;
    daemon
        .review
        .open("chat-1", &[tmp.path().to_path_buf()])
        .await
        .unwrap();
    let root = daemon.review.ledger("chat-1").unwrap().session_base()[0]
        .root
        .clone();
    let source = record("chat-1");
    let mut on_two = request(&source, "a.md", CommentSide::Current, 2, 3);
    on_two.root = Some(root);
    let comment = daemon.comment(on_two).await.unwrap().comment;

    fs::write(tmp.path().join("a.md"), "one\nnew\ntwo\n").unwrap();
    let listed = daemon.comments(&source).await.unwrap();
    assert_eq!(listed.comments.len(), 1);
    assert_eq!(listed.comments[0].comment.line_range, LineRange::new(3, 4));
    assert!(!listed.comments[0].outdated);

    // With no ledger the daemon knows no text, so each range stays.
    daemon.review.clear_session("chat-1");
    let listed = daemon.comments(&source).await.unwrap();
    assert_eq!(listed.comments[0].comment.line_range, comment.line_range);
    assert!(!listed.comments[0].outdated);
}

#[tokio::test]
async fn a_proposal_holds_its_comments() {
    use crucible_core::file_write::ExpectedBase;
    use crucible_core::proposal::ProposalAuthor;

    let daemon = Daemon::new();
    let kiln = TempDir::new().unwrap();
    let root = PhysicalRoot::from_top_level(kiln.path());
    let proposal = daemon
        .proposals
        .record_write(
            ProposalAuthor::Plugin {
                name: "reflection".into(),
            },
            &SessionId::parse("aux-1").unwrap(),
            root.clone(),
            "a.md",
            ExpectedBase::Absent,
            "one\ntwo\n".into(),
        )
        .unwrap();
    let source = DiffsetSource::Proposal { id: proposal.id };

    // A proposal file needs its root, as a session record file does.
    let error = daemon
        .comment(request(&source, "a.md", CommentSide::Current, 2, 3))
        .await
        .unwrap_err();
    assert_eq!(error.code, INVALID_PARAMS);
    let error = daemon
        .comment(DiffCommentRequest {
            root: Some(root.clone()),
            ..request(&source, "other.md", CommentSide::Current, 1, 2)
        })
        .await
        .unwrap_err();
    assert_eq!(error.code, INVALID_PARAMS);

    let reply = daemon
        .comment(DiffCommentRequest {
            root: Some(root.clone()),
            ..request(&source, "a.md", CommentSide::Current, 2, 3)
        })
        .await
        .unwrap();
    assert_eq!(reply.diffset, source.id());
    assert_eq!(reply.comment.anchor, CommentAnchor::Proposal(proposal.id));
    assert_eq!(reply.comment.quoted, "two\n");

    let listed = daemon.comments(&source).await.unwrap();
    assert_eq!(listed.diffset, source.id());
    assert_eq!(listed.comments.len(), 1);
    assert!(!listed.comments[0].outdated);

    daemon.resolve(&source, &reply.comment.id).await.unwrap();
    let listed = daemon.comments(&source).await.unwrap();
    assert!(listed.comments[0].comment.resolved);
}
