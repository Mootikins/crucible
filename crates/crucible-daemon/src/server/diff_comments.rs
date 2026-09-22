//! The `diff.comment`, `diff.resolve_comment` and `diff.comments` RPCs.
//!
//! A diffset owns its comments. Each RPC names the diffset by its source, so
//! the daemon admits a branch root before it reads or writes a comment of
//! that branch. The comment store keys each comment by the diffset id.
//!
//! The Lua bridge calls the same handlers for `cru.diff.comment` and
//! `cru.diff.resolve_comment`, so an agent comment and a person's comment
//! take one path.

use std::collections::HashMap;

use tokio::sync::broadcast;

use crucible_core::diff::{DiffFileText, DiffsetId, DiffsetSource, FileStatus};
use crucible_core::proposal::ProposalId;
use crucible_core::session::{
    Comment, CommentAnchor, CommentAuthor, CommentSide, LineRange, PhysicalRoot, SessionId,
};

use crate::diff::branch;
use crate::diff::comments::{quoted_lines, ListedComment};
use crate::proposals::{ProposalError, ProposalStore};
use crate::protocol::{Request, Response, SessionEventMessage};
use crate::review::{ReviewError, ReviewLedgers, ReviewResult};
use crate::rpc_client::{
    DiffCommentReply, DiffCommentRequest, DiffCommentsReply, DiffCommentsRequest,
    DiffResolveCommentReply, DiffResolveCommentRequest,
};
use crate::rpc_helpers::typed_params;
use crate::server::diff::{
    answer, branch_file_text, branch_sides, check_contained, check_path, internal_error,
    params_error, proposal_file_root, proposal_refusal, Admission, BranchSides, Refusal,
};
use crate::server::session::review::emit_review_changed;

/// A review error as an RPC refusal.
///
/// The caller can correct each variant except the daemon faults: git, I/O,
/// the journal and a snapshot read through the wrong backend.
pub(crate) fn review_refusal(error: ReviewError) -> Refusal {
    match error {
        ReviewError::Git(_)
        | ReviewError::Io(_)
        | ReviewError::Journal { .. }
        | ReviewError::WrongBackend { .. } => internal_error(error),
        ReviewError::NoLedger(_)
        | ReviewError::NoTrackableRoots(_)
        | ReviewError::InvalidSession(_)
        | ReviewError::UnknownComment(_)
        | ReviewError::InvalidComment(_)
        | ReviewError::NotAGitRepo { .. }
        | ReviewError::AmbiguousPath { .. }
        | ReviewError::PathEscapesRoot { .. } => params_error(error.to_string()),
    }
}

/// The text of one side of a file.
pub(crate) fn side_text(texts: &DiffFileText, side: CommentSide) -> Option<&str> {
    match side {
        CommentSide::Base => texts.base_text.as_deref(),
        CommentSide::Current => texts.current_text.as_deref(),
    }
}

/// The parts of a new comment that the source of its diffset decides.
struct Anchored {
    diffset: DiffsetId,
    anchor: CommentAnchor,
    root: PhysicalRoot,
}

/// The fields of a new comment that the caller decides.
pub(crate) struct CommentSpec<'a> {
    pub(crate) path: &'a str,
    pub(crate) side: CommentSide,
    pub(crate) range: LineRange,
    pub(crate) body: &'a str,
    pub(crate) author: CommentAuthor,
}

/// Build a comment and quote its range from the text of its side.
///
/// A side with no text (an absent, binary or too-large file) quotes
/// nothing, so a later listing marks the comment outdated.
fn build(
    anchored: Anchored,
    spec: &CommentSpec<'_>,
    texts: &DiffFileText,
) -> ReviewResult<Comment> {
    let range = spec.range;
    if range.start == 0 || range.is_empty() {
        return Err(ReviewError::InvalidComment(format!(
            "a comment range must start at line 1 or later and end after it starts: {}..{}",
            range.start, range.end
        )));
    }
    let quoted = side_text(texts, spec.side)
        .map(|text| quoted_lines(text, range))
        .unwrap_or_default();
    Ok(Comment::new(
        anchored.diffset,
        anchored.anchor,
        anchored.root,
        spec.path,
        spec.side,
        range,
        quoted,
        spec.body,
        spec.author,
    ))
}

/// Store a comment on a file of the record of `session`.
///
/// The anchor is the session base snapshot of `root`. The daemon tells the
/// clients of the session with `review_changed`, because the review panel
/// of the session shows these comments.
pub(crate) async fn record_comment(
    review: &ReviewLedgers,
    event_tx: &broadcast::Sender<SessionEventMessage>,
    session: &SessionId,
    root: &PhysicalRoot,
    spec: &CommentSpec<'_>,
) -> ReviewResult<Comment> {
    let ledger = review
        .ledger(session.as_str())
        .ok_or_else(|| ReviewError::NoLedger(session.to_string()))?;
    let base_tree = ledger
        .session_base()
        .iter()
        .find(|b| &b.root == root)
        .map(|b| b.base_tree.clone())
        .ok_or_else(|| ReviewError::PathEscapesRoot {
            path: root.to_path_buf(),
        })?;
    let texts = review
        .record_text(session.as_str(), root, spec.path)
        .await?;
    let anchored = Anchored {
        diffset: DiffsetId::for_session(session),
        anchor: CommentAnchor::Snapshot(base_tree),
        root: root.clone(),
    };
    let comment = build(anchored, spec, &texts)?;
    review.add_comment(&comment)?;
    emit_review_changed(event_tx, session.as_str(), "commented");
    Ok(comment)
}

/// Mark a comment of the diffset `diffset` resolved.
///
/// For a session record, the daemon tells the clients of the session.
pub(crate) fn resolve_in(
    review: &ReviewLedgers,
    event_tx: &broadcast::Sender<SessionEventMessage>,
    diffset: &DiffsetId,
    session: Option<&SessionId>,
    comment_id: &str,
) -> ReviewResult<()> {
    review.resolve_diffset_comment(diffset, comment_id)?;
    if let Some(session) = session {
        emit_review_changed(event_tx, session.as_str(), "comment_resolved");
    }
    Ok(())
}

/// A source whose comments the daemon serves, with a branch resolved.
pub(crate) enum Served {
    Branch(BranchSides),
    SessionRecord(SessionId),
    Proposal(ProposalId),
}

impl Served {
    pub(crate) fn diffset(&self) -> DiffsetId {
        match self {
            Served::Branch(sides) => sides.source().id(),
            Served::SessionRecord(session) => DiffsetId::for_session(session),
            Served::Proposal(id) => DiffsetId::for_proposal(id),
        }
    }

    /// The resolved source. A branch source names its resolved base; the
    /// other kinds are `requested` as the caller sent it.
    pub(crate) fn source(&self, requested: &DiffsetSource) -> DiffsetSource {
        match self {
            Served::Branch(sides) => sides.source(),
            Served::SessionRecord(_) | Served::Proposal(_) => requested.clone(),
        }
    }

    /// The session to tell of a change, for a session record only.
    fn session(&self) -> Option<&SessionId> {
        match self {
            Served::SessionRecord(session) => Some(session),
            Served::Branch(_) | Served::Proposal(_) => None,
        }
    }
}

/// Admit and resolve `source`. A branch source with an empty base names the
/// default branch, so its diffset id is the id of the resolved source.
pub(crate) async fn serve(
    admission: &Admission<'_>,
    source: &DiffsetSource,
) -> Result<Served, Refusal> {
    match source {
        DiffsetSource::Branch { root, base, head } => Ok(Served::Branch(
            branch_sides(admission, root, base, head.as_deref()).await?,
        )),
        DiffsetSource::SessionRecord { session } => {
            // A resumed session keeps its ledger on disk until a read loads it.
            crate::server::session::review::ensure_record_loaded(
                admission.review,
                admission.sessions,
                session.as_str(),
            )
            .await;
            Ok(Served::SessionRecord(session.clone()))
        }
        DiffsetSource::Proposal { id } => {
            admission.proposals.get(id).map_err(proposal_refusal)?;
            Ok(Served::Proposal(*id))
        }
    }
}

async fn diff_comment(
    admission: &Admission<'_>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
    request: &DiffCommentRequest,
) -> Result<DiffCommentReply, Refusal> {
    check_path(&request.path)?;
    let base_path = request.from.as_deref().unwrap_or(&request.path);
    check_path(base_path)?;
    let spec = CommentSpec {
        path: &request.path,
        side: request.side,
        range: LineRange::new(
            request.line_start,
            request.line_end.unwrap_or(request.line_start + 1),
        ),
        body: &request.body,
        author: request.author.unwrap_or(CommentAuthor::Human),
    };
    let served = serve(admission, &request.source).await?;
    let source = served.source(&request.source);
    let comment = match served {
        Served::Branch(sides) => {
            if request.root.as_ref().is_some_and(|r| *r != sides.root) {
                return Err(params_error(
                    "the root of the request is not the root of the branch source",
                ));
            }
            let texts = branch_file_text(&sides, &request.path, base_path).await?;
            let anchored = Anchored {
                diffset: sides.source().id(),
                anchor: CommentAnchor::Commit(sides.merge_base.clone()),
                root: sides.root.clone(),
            };
            let comment = build(anchored, &spec, &texts).map_err(review_refusal)?;
            admission
                .review
                .add_comment(&comment)
                .map_err(review_refusal)?;
            comment
        }
        Served::SessionRecord(session) => {
            let Some(root) = &request.root else {
                return Err(params_error("a session record file needs its root"));
            };
            if request.from.is_some() {
                return Err(params_error("a session record has no renamed file"));
            }
            check_contained(root, &request.path)?;
            record_comment(admission.review, event_tx, &session, root, &spec)
                .await
                .map_err(review_refusal)?
        }
        Served::Proposal(id) => {
            let root = proposal_file_root(request.root.as_ref(), request.from.as_deref())?;
            let texts = admission
                .proposals
                .diff_text(&id, root, &request.path)
                .map_err(proposal_refusal)?;
            let anchored = Anchored {
                diffset: DiffsetId::for_proposal(&id),
                anchor: CommentAnchor::Proposal(id),
                root: root.clone(),
            };
            let comment = build(anchored, &spec, &texts).map_err(review_refusal)?;
            admission
                .review
                .add_comment(&comment)
                .map_err(review_refusal)?;
            comment
        }
    };
    // A search by comment id needs the source: a branch id does not name it.
    admission
        .review
        .comment_store()
        .remember_source(&source)
        .map_err(internal_error)?;
    Ok(DiffCommentReply {
        diffset: comment.diffset.clone(),
        comment,
    })
}

async fn diff_resolve_comment(
    admission: &Admission<'_>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
    request: &DiffResolveCommentRequest,
) -> Result<DiffResolveCommentReply, Refusal> {
    let served = serve(admission, &request.source).await?;
    let diffset = served.diffset();
    resolve_in(
        admission.review,
        event_tx,
        &diffset,
        served.session(),
        &request.comment_id,
    )
    .map_err(review_refusal)?;
    Ok(DiffResolveCommentReply {
        diffset,
        comment_id: request.comment_id.clone(),
        resolved: true,
    })
}

/// The file of a comment on one side: the key of the text cache.
type SideKey = (bool, PhysicalRoot, String);

fn side_key(comment: &Comment) -> SideKey {
    (
        comment.side == CommentSide::Base,
        comment.root.clone(),
        comment.path.clone(),
    )
}

/// The current text of each side that a comment of `sides` counts on.
async fn branch_texts(
    sides: &BranchSides,
    comments: &[Comment],
) -> Result<HashMap<SideKey, Option<String>>, Refusal> {
    // The old path of each renamed file. A base-side comment of a renamed
    // file quotes the old path, so its base text is at the old path.
    let mut renames: Option<HashMap<String, String>> = None;
    let mut texts = HashMap::new();
    for comment in comments {
        let key = side_key(comment);
        if texts.contains_key(&key) {
            continue;
        }
        let path = comment.path.as_str();
        let text = match comment.side {
            CommentSide::Current => {
                if sides.head.is_none() && check_contained(&sides.root, path).is_err() {
                    None
                } else {
                    branch::file_text(&sides.root, sides.head.as_deref(), path)
                        .await
                        .map_err(internal_error)?
                        .into_shown()
                }
            }
            CommentSide::Base => {
                if renames.is_none() {
                    let changes =
                        branch::changes(&sides.root, &sides.merge_base, sides.head.as_deref())
                            .await
                            .map_err(internal_error)?;
                    renames = Some(
                        changes
                            .into_iter()
                            .filter_map(|entry| match entry.status {
                                FileStatus::Renamed { from } => Some((entry.path, from)),
                                _ => None,
                            })
                            .collect(),
                    );
                }
                let base_path = renames
                    .as_ref()
                    .and_then(|r| r.get(path))
                    .map_or(path, String::as_str);
                branch::file_text(&sides.root, Some(&sides.merge_base), base_path)
                    .await
                    .map_err(internal_error)?
                    .into_shown()
            }
        };
        texts.insert(key, text);
    }
    Ok(texts)
}

/// The current text of each side that a comment of a session record counts
/// on. `None` when the session has no ledger: then no text is known.
async fn record_texts(
    review: &ReviewLedgers,
    session: &SessionId,
    comments: &[Comment],
) -> Result<Option<HashMap<SideKey, Option<String>>>, Refusal> {
    if review.ledger(session.as_str()).is_none() {
        return Ok(None);
    }
    let mut files: HashMap<(PhysicalRoot, String), DiffFileText> = HashMap::new();
    let mut texts = HashMap::new();
    for comment in comments {
        let file = (comment.root.clone(), comment.path.clone());
        if !files.contains_key(&file) {
            let text = match review
                .record_text(session.as_str(), &comment.root, &comment.path)
                .await
            {
                Ok(text) => text,
                // The session no longer tracks the root of the comment.
                Err(ReviewError::PathEscapesRoot { .. }) => DiffFileText {
                    base_text: None,
                    current_text: None,
                },
                Err(e) => return Err(review_refusal(e)),
            };
            files.insert(file.clone(), text);
        }
        let text = side_text(&files[&file], comment.side).map(str::to_string);
        texts.insert(side_key(comment), text);
    }
    Ok(Some(texts))
}

/// The current text of each side that a comment of a proposal counts on. A
/// file that the proposal no longer writes has no text, so its comments are
/// outdated.
fn proposal_texts(
    proposals: &ProposalStore,
    id: &ProposalId,
    comments: &[Comment],
) -> Result<HashMap<SideKey, Option<String>>, Refusal> {
    let mut texts = HashMap::new();
    for comment in comments {
        let key = side_key(comment);
        if texts.contains_key(&key) {
            continue;
        }
        let text = match proposals.diff_text(id, &comment.root, &comment.path) {
            Ok(text) => side_text(&text, comment.side).map(str::to_string),
            Err(ProposalError::NoWrite(..)) => None,
            Err(e) => return Err(proposal_refusal(e)),
        };
        texts.insert(key, text);
    }
    Ok(texts)
}

async fn diff_comments(
    admission: &Admission<'_>,
    request: &DiffCommentsRequest,
) -> Result<DiffCommentsReply, Refusal> {
    let served = serve(admission, &request.source).await?;
    let diffset = served.diffset();
    let store = admission.review.comment_store();
    let stored = store.list(&diffset).map_err(internal_error)?;
    let texts = match &served {
        Served::Branch(sides) => Some(branch_texts(sides, &stored).await?),
        Served::SessionRecord(session) => record_texts(admission.review, session, &stored).await?,
        Served::Proposal(id) => Some(proposal_texts(admission.proposals, id, &stored)?),
    };
    let comments = match texts {
        Some(texts) => store
            .list_projected(&diffset, |c| texts.get(&side_key(c)).cloned().flatten())
            .map_err(internal_error)?,
        // With no known text, each comment keeps its stored range.
        None => stored
            .into_iter()
            .map(|comment| ListedComment {
                comment,
                outdated: false,
            })
            .collect(),
    };
    Ok(DiffCommentsReply { diffset, comments })
}

/// Handle the `diff.comment` RPC.
pub(crate) async fn handle_diff_comment(
    req: Request,
    admission: Admission<'_>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let params = match typed_params::<DiffCommentRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    answer(req.id, diff_comment(&admission, event_tx, &params).await)
}

/// Handle the `diff.resolve_comment` RPC.
pub(crate) async fn handle_diff_resolve_comment(
    req: Request,
    admission: Admission<'_>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let params = match typed_params::<DiffResolveCommentRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    answer(
        req.id,
        diff_resolve_comment(&admission, event_tx, &params).await,
    )
}

/// Handle the `diff.comments` RPC.
pub(crate) async fn handle_diff_comments(req: Request, admission: Admission<'_>) -> Response {
    let params = match typed_params::<DiffCommentsRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    answer(req.id, diff_comments(&admission, &params).await)
}

#[cfg(test)]
#[path = "diff_comments_tests.rs"]
mod tests;
