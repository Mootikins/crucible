//! Resolve the review comments that a chat message attaches.
//!
//! A client sends references only: a [`CommentRef`] (id and diffset source)
//! from the web composer, or an `@comment:<id>` mention in the text from the
//! TUI. The daemon finds each comment, reads the two texts of its file in
//! its diffset and builds the block of [`crate::diff::context`]. The send
//! path then injects the blocks as context before the user turn.
//!
//! A reference to a comment that the daemon cannot find, or to a resolved
//! comment, refuses the message. The daemon does not drop it in silence.

use std::path::Path;

use crucible_core::diff::{
    project, CommentRef, DiffFileText, DiffsetSource, FileStatus, Projection,
};
use crucible_core::session::Comment;

use crate::diff::branch;
use crate::diff::context::{message, CommentBlock, ReviewContext};
use crate::proposals::ProposalError;
use crate::review::ReviewError;
use crate::server::diff::{
    branch_file_text, internal_error, params_error, proposal_refusal, Admission, Refusal,
};
use crate::server::diff_comments::{review_refusal, serve, side_text, Served};

/// The context message of the comments that `refs` and the `@comment:`
/// mentions of `content` name, or `None` when they name no comment.
///
/// `session` is the session that receives the message, and `workspace` is
/// its workspace. A comment that is named twice attaches once.
pub(crate) async fn review_context(
    admission: &Admission<'_>,
    session: &str,
    workspace: Option<&Path>,
    refs: &[CommentRef],
    content: &str,
) -> Result<Option<ReviewContext>, Refusal> {
    let mut wanted: Vec<(String, Option<DiffsetSource>)> = Vec::new();
    for reference in refs {
        if !wanted.iter().any(|(id, _)| *id == reference.id) {
            wanted.push((reference.id.clone(), Some(reference.source.clone())));
        }
    }
    for id in crate::agent_manager::attachments::comment_mentions(content) {
        if !wanted.iter().any(|(known, _)| known == id) {
            wanted.push((id.to_string(), None));
        }
    }
    if wanted.is_empty() {
        return Ok(None);
    }

    let mut found = Vec::with_capacity(wanted.len());
    for (id, source) in wanted {
        let (comment, source) = find(admission, &id, source).await?;
        if comment.resolved {
            return Err(params_error(format!(
                "comment {id} is resolved; a resolved comment does not attach to a message"
            )));
        }
        let served = serve(admission, &source).await?;
        let texts = texts_of(admission, &served, &comment).await?;
        let (range, outdated) = match side_text(&texts, comment.side) {
            Some(text) => match project(&comment.quoted, comment.line_range, text) {
                Projection::Kept => (comment.line_range, false),
                Projection::Moved(range) => (range, false),
                Projection::Outdated => (comment.line_range, true),
            },
            None => (comment.line_range, true),
        };
        let section = section(&served.source(&source), session);
        found.push((comment, range, outdated, texts, section));
    }
    let blocks: Vec<_> = found
        .iter()
        .map(|(comment, range, outdated, texts, section)| CommentBlock {
            comment,
            range: *range,
            outdated: *outdated,
            texts,
            section,
            workspace,
        })
        .collect();
    Ok(Some(message(&blocks)))
}

/// Find the comment `id`: in the diffset of `source` when the client names
/// one, else in every diffset.
async fn find(
    admission: &Admission<'_>,
    id: &str,
    source: Option<DiffsetSource>,
) -> Result<(Comment, DiffsetSource), Refusal> {
    let store = admission.review.comment_store();
    match source {
        Some(source) => {
            let diffset = serve(admission, &source).await?.diffset();
            let comment = store
                .list(&diffset)
                .map_err(internal_error)?
                .into_iter()
                .find(|c| c.id == id)
                .ok_or_else(|| {
                    params_error(format!("the diffset {diffset} has no comment {id}"))
                })?;
            Ok((comment, source))
        }
        None => match store.find(id).map_err(internal_error)? {
            Some((comment, Some(source))) => Ok((comment, source)),
            Some((comment, None)) => Err(params_error(format!(
                "comment {id} belongs to the diffset {}, whose source the daemon did not keep; \
                 attach it from the diff pane",
                comment.diffset
            ))),
            None => Err(params_error(format!("no stored comment has the id {id}"))),
        },
    }
}

/// The two texts of the file of `comment` in its diffset.
async fn texts_of(
    admission: &Admission<'_>,
    served: &Served,
    comment: &Comment,
) -> Result<DiffFileText, Refusal> {
    match served {
        Served::Branch(sides) => {
            if sides.root != comment.root {
                return Err(params_error(format!(
                    "comment {} names a root that is not the root of its branch",
                    comment.id
                )));
            }
            // The base text of a renamed file is at its old path.
            let changes = branch::changes(&sides.root, &sides.merge_base, sides.head.as_deref())
                .await
                .map_err(internal_error)?;
            let from = changes.into_iter().find_map(|entry| match entry.status {
                FileStatus::Renamed { from } if entry.path == comment.path => Some(from),
                _ => None,
            });
            branch_file_text(
                sides,
                &comment.path,
                from.as_deref().unwrap_or(&comment.path),
            )
            .await
        }
        Served::SessionRecord(session) => match admission
            .review
            .record_text(session.as_str(), &comment.root, &comment.path)
            .await
        {
            Ok(texts) => Ok(texts),
            // The session no longer tracks the root: the comment is outdated.
            Err(ReviewError::PathEscapesRoot { .. } | ReviewError::NoLedger(_)) => {
                Ok(DiffFileText {
                    base_text: None,
                    current_text: None,
                })
            }
            Err(e) => Err(review_refusal(e)),
        },
        Served::Proposal(id) => {
            match admission
                .proposals
                .diff_text(id, &comment.root, &comment.path)
            {
                Ok(texts) => Ok(texts),
                // The proposal no longer writes the file: the comment is outdated.
                Err(ProposalError::NoWrite(..)) => Ok(DiffFileText {
                    base_text: None,
                    current_text: None,
                }),
                Err(e) => Err(proposal_refusal(e)),
            }
        }
    }
}

/// What the diffset of a comment compares, as a phrase for the agent.
fn section(source: &DiffsetSource, session: &str) -> String {
    match source {
        DiffsetSource::Branch { base, head, .. } => format!(
            "Branch changes: {} against {}",
            head.as_deref().unwrap_or("the working tree"),
            if base.is_empty() {
                "the default branch"
            } else {
                base
            }
        ),
        DiffsetSource::SessionRecord { session: owner } if owner.as_str() == session => {
            "Session changes".to_string()
        }
        DiffsetSource::SessionRecord { session: owner } => {
            format!("Session changes of session {owner}")
        }
        DiffsetSource::Proposal { id } => format!("Proposal {id}"),
    }
}
