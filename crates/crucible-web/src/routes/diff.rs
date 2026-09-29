//! The diffset routes: a thin proxy over the daemon's `diff.get`,
//! `diff.file`, `diff.comment`, `diff.resolve_comment`, `diff.delete_comment`
//! and `diff.comments` RPCs.
//!
//! The daemon admits the root, reads the files and stores the comments.
//! These routes only turn the query into a `DiffsetSource`. A query names a
//! branch source with `root`, a session record source with `session`, or a
//! proposal source with `proposal`. A comment write carries the
//! `DiffsetSource` in its JSON body.

use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Query, State},
    Json,
};
use crucible_core::diff::{DiffFileText, Diffset, DiffsetSource};
use crucible_core::proposal::ProposalId;
use crucible_core::protocol::requests::{
    DiffCommentReply, DiffCommentRequest, DiffCommentsReply, DiffDeleteCommentReply,
    DiffFileRequest, DiffResolveCommentReply,
};
use crucible_core::session::{CommentAuthor, CommentSide, PhysicalRoot, SessionId};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

pub fn diff_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_diff))
        .routes(routes!(get_diff_file))
        .routes(routes!(post_diff_comment))
        .routes(routes!(post_diff_resolve_comment))
        .routes(routes!(post_diff_delete_comment))
        .routes(routes!(get_diff_comments))
}

/// The session record source of `session`.
///
/// A session record has no branch, so a `base` or a `head` beside it is a
/// mistake of the caller.
fn session_record(
    session: &str,
    base: Option<&str>,
    head: Option<&str>,
) -> Result<DiffsetSource, WebError> {
    if base.is_some() || head.is_some() {
        return Err(WebError::Validation(
            "a session record takes no base and no head".into(),
        ));
    }
    let session = SessionId::parse(session).map_err(|e| WebError::Validation(e.to_string()))?;
    Ok(DiffsetSource::SessionRecord { session })
}

/// The proposal source of `id`. A proposal has no branch either.
fn proposal(id: &str, base: Option<&str>, head: Option<&str>) -> Result<DiffsetSource, WebError> {
    if base.is_some() || head.is_some() {
        return Err(WebError::Validation(
            "a proposal takes no base and no head".into(),
        ));
    }
    let id: ProposalId = id
        .parse()
        .map_err(|_| WebError::Validation(format!("not a proposal id: {id:?}")))?;
    Ok(DiffsetSource::Proposal { id })
}

fn branch(root: String, base: Option<String>, head: Option<String>) -> DiffsetSource {
    DiffsetSource::Branch {
        // The daemon resolves the top level and refuses any other path.
        root: PhysicalRoot::from_top_level(root),
        base: base.unwrap_or_default(),
        head,
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct DiffQuery {
    /// Absolute path of the top level of a git repository. Names a branch
    /// source. Give exactly one of `root`, `session` and `proposal`.
    root: Option<String>,
    /// The id of a session. Names the record of the session: its session
    /// base, to the files on disk.
    session: Option<String>,
    /// The id of a proposal. Names the proposal: the base of each write, to
    /// its new text.
    proposal: Option<String>,
    /// The branch to compare against. Absent takes the default branch.
    base: Option<String>,
    /// The other side. Absent takes the working tree.
    head: Option<String>,
}

impl DiffQuery {
    fn source(self) -> Result<DiffsetSource, WebError> {
        match (self.root, self.session, self.proposal) {
            (Some(root), None, None) => Ok(branch(root, self.base, self.head)),
            (None, Some(session), None) => {
                session_record(&session, self.base.as_deref(), self.head.as_deref())
            }
            (None, None, Some(id)) => proposal(&id, self.base.as_deref(), self.head.as_deref()),
            _ => Err(WebError::Validation(
                "give exactly one of root, session and proposal".into(),
            )),
        }
    }
}

/// `GET /api/diff` — the files of the branch diff of one root, of the record
/// of one session, or of one proposal.
///
/// The reply has the counts of each file and no text. For a branch, its
/// `source` names the base branch that the daemon used. A session with no
/// review ledger has no files.
#[utoipa::path(
    get,
    path = "/api/diff",
    params(DiffQuery),
    responses(
        (status = 200, body = Diffset),
        (status = 422, description = "The query or the daemon refuses the source, and says why"),
        (status = 502, description = "git failed, or the daemon could not be reached"),
    )
)]
async fn get_diff(
    State(state): State<AppState>,
    Query(query): Query<DiffQuery>,
) -> Result<Json<Diffset>, WebError> {
    let diffset = state.daemon.diff_get(&query.source()?).await.daemon_err()?;
    Ok(Json(diffset))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct FileQuery {
    /// Absolute path of the root of the file. For a branch source, the top
    /// level of the git repository. For a session record or a proposal, the
    /// `root` of the file entry.
    root: String,
    /// The id of a session. Names the record of the session instead of a
    /// branch.
    session: Option<String>,
    /// The id of a proposal. Names the proposal instead of a branch. Give
    /// `session` or `proposal`, not both.
    proposal: Option<String>,
    /// The branch to compare against. Absent takes the default branch.
    base: Option<String>,
    /// The other side. Absent takes the working tree.
    head: Option<String>,
    /// The path of the file relative to `root`, on the current side.
    path: String,
    /// The old path of a renamed file.
    from: Option<String>,
}

impl FileQuery {
    fn request(self) -> Result<DiffFileRequest, WebError> {
        let FileQuery {
            root,
            session,
            proposal: proposal_id,
            base,
            head,
            path,
            from,
        } = self;
        let (source, root) = match (session, proposal_id) {
            (Some(session), None) => (
                session_record(&session, base.as_deref(), head.as_deref())?,
                Some(PhysicalRoot::from_top_level(root)),
            ),
            (None, Some(id)) => (
                proposal(&id, base.as_deref(), head.as_deref())?,
                Some(PhysicalRoot::from_top_level(root)),
            ),
            // The branch source names its own root.
            (None, None) => (branch(root, base, head), None),
            (Some(_), Some(_)) => {
                return Err(WebError::Validation(
                    "give session or proposal, not both".into(),
                ))
            }
        };
        Ok(DiffFileRequest {
            source,
            path,
            from,
            root,
        })
    }
}

/// `GET /api/diff/file` — the two texts of one file of a branch diff, of a
/// session record or of a proposal.
///
/// A side is `null` when the file is absent on it, binary or too large.
#[utoipa::path(
    get,
    path = "/api/diff/file",
    params(FileQuery),
    responses(
        (status = 200, body = DiffFileText),
        (status = 422, description = "The query or the daemon refuses the source, the root or the path, and says why"),
        (status = 502, description = "git failed, or the daemon could not be reached"),
    )
)]
async fn get_diff_file(
    State(state): State<AppState>,
    Query(query): Query<FileQuery>,
) -> Result<Json<DiffFileText>, WebError> {
    let text = state
        .daemon
        .diff_file_request(&query.request()?)
        .await
        .daemon_err()?;
    Ok(Json(text))
}

/// `POST /api/diff/comment` — anchor a comment to a line range of one file
/// of a diffset.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct CommentBody {
    /// The diffset of the file.
    source: DiffsetSource,
    /// Absolute path of the root of the file. A session record and a
    /// proposal need it, because each can have more than one root. A branch
    /// source names its own root.
    #[serde(default)]
    root: Option<String>,
    /// The path of the file relative to the root, on the current side.
    path: String,
    /// The old path of a renamed file. A base-side comment quotes this path.
    #[serde(default)]
    from: Option<String>,
    /// The side that the line numbers count on.
    side: CommentSide,
    /// The first line, 1-based.
    line_start: u32,
    /// One past the last line. Absent means one line.
    #[serde(default)]
    line_end: Option<u32>,
    body: String,
    /// Absent means a human.
    #[serde(default)]
    author: Option<CommentAuthor>,
}

impl From<CommentBody> for DiffCommentRequest {
    fn from(body: CommentBody) -> Self {
        DiffCommentRequest {
            source: body.source,
            root: body.root.map(PhysicalRoot::from_top_level),
            path: body.path,
            from: body.from,
            side: body.side,
            line_start: body.line_start,
            line_end: body.line_end,
            body: body.body,
            author: body.author,
        }
    }
}

/// `POST /api/diff/comment/resolve` — mark one comment of a diffset
/// resolved.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct ResolveCommentBody {
    /// The diffset of the comment.
    source: DiffsetSource,
    comment_id: String,
}

/// `POST /api/diff/comment/delete` — remove one comment of a diffset from
/// the store.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct DeleteCommentBody {
    /// The diffset of the comment.
    source: DiffsetSource,
    comment_id: String,
}

/// `POST /api/diff/comment` — anchor a comment to a line range of one file of
/// a branch diff or of a session record.
///
/// The daemon quotes the text of the range on the named side. A comment on a
/// session record also tells the clients of the session with
/// `review_changed`.
#[utoipa::path(
    post,
    path = "/api/diff/comment",
    request_body = CommentBody,
    responses(
        (status = 200, body = DiffCommentReply),
        (status = 422, description = "The daemon refuses the source, the root, the path or the range, and says why"),
        (status = 502, description = "The daemon could not store the comment, or could not be reached"),
    )
)]
async fn post_diff_comment(
    State(state): State<AppState>,
    Json(body): Json<CommentBody>,
) -> Result<Json<DiffCommentReply>, WebError> {
    let reply = state.daemon.diff_comment(&body.into()).await.daemon_err()?;
    Ok(Json(reply))
}

/// `POST /api/diff/comment/resolve` — mark one comment of a diffset resolved.
#[utoipa::path(
    post,
    path = "/api/diff/comment/resolve",
    request_body = ResolveCommentBody,
    responses(
        (status = 200, body = DiffResolveCommentReply),
        (status = 422, description = "The diffset has no such comment, or the daemon refuses the source"),
        (status = 502, description = "The daemon could not resolve the comment, or could not be reached"),
    )
)]
async fn post_diff_resolve_comment(
    State(state): State<AppState>,
    Json(body): Json<ResolveCommentBody>,
) -> Result<Json<DiffResolveCommentReply>, WebError> {
    let reply = state
        .daemon
        .diff_resolve_comment(&body.source, &body.comment_id)
        .await
        .daemon_err()?;
    Ok(Json(reply))
}

/// `POST /api/diff/comment/delete` — remove one comment of a diffset.
///
/// Delete is not resolve. Resolve keeps a settled remark in the record;
/// delete says that the author never wrote the remark, so the comment leaves
/// the store.
#[utoipa::path(
    post,
    path = "/api/diff/comment/delete",
    request_body = DeleteCommentBody,
    responses(
        (status = 200, body = DiffDeleteCommentReply),
        (status = 422, description = "The diffset has no such comment, or the daemon refuses the source"),
        (status = 502, description = "The daemon could not delete the comment, or could not be reached"),
    )
)]
async fn post_diff_delete_comment(
    State(state): State<AppState>,
    Json(body): Json<DeleteCommentBody>,
) -> Result<Json<DiffDeleteCommentReply>, WebError> {
    let reply = state
        .daemon
        .diff_delete_comment(&body.source, &body.comment_id)
        .await
        .daemon_err()?;
    Ok(Json(reply))
}

/// `GET /api/diff/comments` — the comments of the branch diff of one root, of
/// the record of one session, or of one proposal.
///
/// The daemon finds the quoted text of each comment in the current text of
/// its side. A moved text moves the range. A text that is gone makes the
/// comment outdated.
#[utoipa::path(
    get,
    path = "/api/diff/comments",
    params(DiffQuery),
    responses(
        (status = 200, body = DiffCommentsReply),
        (status = 422, description = "The query or the daemon refuses the source, and says why"),
        (status = 502, description = "git failed, or the daemon could not be reached"),
    )
)]
async fn get_diff_comments(
    State(state): State<AppState>,
    Query(query): Query<DiffQuery>,
) -> Result<Json<DiffCommentsReply>, WebError> {
    let reply = state
        .daemon
        .diff_comments(&query.source()?)
        .await
        .daemon_err()?;
    Ok(Json(reply))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        mock_diff_comment_for, mock_diff_file_text_for, mock_diffset_for, request_json, shape,
    };

    /// The query reaches the daemon as a branch source, and the reply reaches
    /// the browser as the daemon wrote it. An absent base is the empty string,
    /// which asks the daemon for the default branch.
    #[tokio::test]
    async fn get_diff_answers_the_declared_shape() {
        let answered: Diffset = shape("GET", "/api/diff?root=/tmp/test-project", None).await;
        let source = DiffsetSource::Branch {
            root: PhysicalRoot::from_top_level("/tmp/test-project"),
            base: String::new(),
            head: None,
        };
        assert_eq!(answered, mock_diffset_for(source));

        let text: DiffFileText = shape(
            "GET",
            "/api/diff/file?root=/tmp/test-project&base=main&head=topic&path=new.md&from=old.md",
            None,
        )
        .await;
        let request = DiffFileRequest {
            source: DiffsetSource::Branch {
                root: PhysicalRoot::from_top_level("/tmp/test-project"),
                base: "main".into(),
                head: Some("topic".into()),
            },
            path: "new.md".into(),
            from: Some("old.md".into()),
            root: None,
        };
        assert_eq!(text, mock_diff_file_text_for(&request));
    }

    /// A `session` query reaches the daemon as a session record source. The
    /// file request carries the root of the file.
    #[tokio::test]
    async fn a_session_query_names_the_session_record() {
        let source = DiffsetSource::SessionRecord {
            session: SessionId::parse("chat-1").unwrap(),
        };
        let answered: Diffset = shape("GET", "/api/diff?session=chat-1", None).await;
        assert_eq!(answered, mock_diffset_for(source.clone()));

        let text: DiffFileText = shape(
            "GET",
            "/api/diff/file?session=chat-1&root=/tmp/test-project&path=a.md",
            None,
        )
        .await;
        let request = DiffFileRequest {
            source,
            path: "a.md".into(),
            from: None,
            root: Some(PhysicalRoot::from_top_level("/tmp/test-project")),
        };
        assert_eq!(text, mock_diff_file_text_for(&request));
    }

    /// The comment body reaches the daemon as a typed request: the source,
    /// the root, the side, the range and the author. The reply reaches the
    /// browser with the stored comment.
    #[tokio::test]
    async fn post_diff_comment_answers_the_declared_shape() {
        let source = DiffsetSource::SessionRecord {
            session: SessionId::parse("chat-1").unwrap(),
        };
        let body = serde_json::json!({
            "source": source,
            "root": "/tmp/test-project",
            "path": "a.md",
            "side": "base",
            "line_start": 2,
            "line_end": 4,
            "body": "why?",
            "author": "agent",
        });
        let answered: DiffCommentReply = shape("POST", "/api/diff/comment", Some(body)).await;
        let request = DiffCommentRequest {
            source: source.clone(),
            root: Some(PhysicalRoot::from_top_level("/tmp/test-project")),
            path: "a.md".into(),
            from: None,
            side: crucible_core::session::CommentSide::Base,
            line_start: 2,
            line_end: Some(4),
            body: "why?".into(),
            author: Some(crucible_core::session::CommentAuthor::Agent),
        };
        let expected = mock_diff_comment_for(&request);
        assert_eq!(answered.diffset, source.id());
        // The row writes back the comment that the daemon stored, except
        // the id and the time that the mock minted for each call.
        let mut written = serde_json::to_value(&answered.comment).unwrap();
        let mut stored = serde_json::to_value(&expected.comment).unwrap();
        for value in [&mut written, &mut stored] {
            let object = value.as_object_mut().unwrap();
            object.remove("id");
            object.remove("created_at");
        }
        assert_eq!(written, stored);

        // A key that the route does not know is refused.
        let (status, _) = request_json(
            "POST",
            "/api/diff/comment",
            Some(serde_json::json!({
                "source": source, "path": "a.md", "side": "current",
                "line_start": 1, "body": "x", "session_id": "other",
            })),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn post_diff_resolve_comment_answers_the_declared_shape() {
        let source = DiffsetSource::Branch {
            root: PhysicalRoot::from_top_level("/tmp/test-project"),
            base: "main".into(),
            head: None,
        };
        let answered: DiffResolveCommentReply = shape(
            "POST",
            "/api/diff/comment/resolve",
            Some(serde_json::json!({ "source": source, "comment_id": "comment-1" })),
        )
        .await;
        assert_eq!(answered.diffset, source.id());
        assert_eq!(answered.comment_id, "comment-1");
        assert!(answered.resolved);
    }

    #[tokio::test]
    async fn post_diff_delete_comment_answers_the_declared_shape() {
        let source = DiffsetSource::Branch {
            root: PhysicalRoot::from_top_level("/tmp/test-project"),
            base: "main".into(),
            head: None,
        };
        let answered: DiffDeleteCommentReply = shape(
            "POST",
            "/api/diff/comment/delete",
            Some(serde_json::json!({ "source": source, "comment_id": "comment-1" })),
        )
        .await;
        assert_eq!(answered.diffset, source.id());
        assert_eq!(answered.comment_id, "comment-1");
        assert!(answered.deleted);
    }

    /// The query names the source as on `GET /api/diff`, and the
    /// `outdated` flag of each comment reaches the browser.
    #[tokio::test]
    async fn get_diff_comments_answers_the_declared_shape() {
        let answered: DiffCommentsReply =
            shape("GET", "/api/diff/comments?session=chat-1", None).await;
        let source = DiffsetSource::SessionRecord {
            session: SessionId::parse("chat-1").unwrap(),
        };
        assert_eq!(answered.diffset, source.id());
        assert_eq!(answered.comments.len(), 1);
        assert!(answered.comments[0].outdated);

        let (status, _) = request_json("GET", "/api/diff/comments", None).await;
        assert_eq!(status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
    }

    /// A `proposal` query reaches the daemon as a proposal source. The file
    /// request carries the root of the file.
    #[tokio::test]
    async fn a_proposal_query_names_the_proposal() {
        let id = crate::test_support::mock_proposal_id();
        let source = DiffsetSource::Proposal { id };
        let answered: Diffset = shape("GET", &format!("/api/diff?proposal={id}"), None).await;
        assert_eq!(answered, mock_diffset_for(source.clone()));

        let text: DiffFileText = shape(
            "GET",
            &format!("/api/diff/file?proposal={id}&root=/tmp/test-project&path=a.md"),
            None,
        )
        .await;
        let request = DiffFileRequest {
            source: source.clone(),
            path: "a.md".into(),
            from: None,
            root: Some(PhysicalRoot::from_top_level("/tmp/test-project")),
        };
        assert_eq!(text, mock_diff_file_text_for(&request));

        let answered: DiffCommentsReply =
            shape("GET", &format!("/api/diff/comments?proposal={id}"), None).await;
        assert_eq!(answered.diffset, source.id());
    }

    #[tokio::test]
    async fn a_query_without_one_source_is_refused() {
        for uri in [
            "/api/diff",
            "/api/diff?root=/tmp/test-project&session=chat-1",
            "/api/diff?session=chat-1&base=main",
            "/api/diff?session=chat-1&proposal=6f1c1d2e-3b4a-4c5d-8e9f-0a1b2c3d4e5f",
            "/api/diff?proposal=not-a-uuid",
            "/api/diff?proposal=6f1c1d2e-3b4a-4c5d-8e9f-0a1b2c3d4e5f&head=topic",
            "/api/diff/file?session=chat-1&root=/tmp/test-project&path=a.md&head=topic",
        ] {
            let (status, _) = request_json("GET", uri, None).await;
            assert_eq!(
                status,
                axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                "{uri}"
            );
        }
    }
}
