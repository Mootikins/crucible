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
//!
//! Rust owns the template. The `review_comment_format` stage lets an
//! operator replace the BODY of a block, and the host puts the reply back
//! between the same two tags. The shipped defaults register no handler, so
//! the text of [`crate::diff::context`] stands until somebody asks for
//! another one.

use std::ops::ControlFlow;
use std::path::Path;

use crucible_core::diff::{
    project, CommentRef, DiffFileText, DiffsetSource, FileStatus, Projection,
};
use crucible_core::events::SessionEvent;
use crucible_core::session::Comment;
use crucible_lua::StageId;
use tracing::warn;

use crate::agent_manager::vm_pass::{run_handlers, PluginHandlers};
use crate::diff::branch;
use crate::diff::context::{message, render, CommentBlock, Rendered};
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
/// its workspace. A comment that is named twice attaches once. `handlers`
/// is the plugin VM, which the `review_comment_format` stage runs in.
pub(crate) async fn review_context(
    admission: &Admission<'_>,
    session: &str,
    workspace: Option<&Path>,
    refs: &[CommentRef],
    content: &str,
    handlers: Option<&PluginHandlers>,
) -> Result<Option<String>, Refusal> {
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

    let mut blocks = Vec::with_capacity(wanted.len());
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
        let rendered = render(&CommentBlock {
            comment: &comment,
            range,
            outdated,
            texts: &texts,
            section: &section,
            workspace,
        });
        blocks.push(formatted(handlers, session, &rendered).await);
    }
    Ok(Some(message(&blocks)))
}

/// The block of `rendered`, with the body that the `review_comment_format`
/// stage answers when one does.
///
/// With no handler the answer is byte for byte what [`render`] built. Rust
/// owns the default template; this stage rewrites the rendered text only.
async fn formatted(
    handlers: Option<&PluginHandlers>,
    session: &str,
    rendered: &Rendered,
) -> String {
    let event = SessionEvent::Custom {
        name: StageId::ReviewCommentFormat.as_str().to_string(),
        payload: rendered.payload(),
    };
    run_handlers(handlers, rendered.text(), |registry, lua, text| {
        let event = &event;
        Box::pin(async move {
            match body_of_first_handler(session, &registry, &lua, event).await {
                Some(body) => ControlFlow::Break(rendered.wrap(&body)),
                None => ControlFlow::Continue(text),
            }
        })
    })
    .await
}

/// The body that this registry's first `review_comment_format` handler
/// answers, or `None` when none answers a string.
///
/// The first Transform wins, as at `precognition_format`: the payload names
/// the body Rust built, and a second handler would read a body that is no
/// longer the one in the block. A handler that raises is logged and skipped,
/// so a broken plugin costs the operator their wording and not their turn.
async fn body_of_first_handler(
    session: &str,
    registry: &crucible_lua::LuaScriptHandlerRegistry,
    lua: &mlua::Lua,
    event: &SessionEvent,
) -> Option<String> {
    use crucible_lua::ScriptHandlerResult;

    for handler in registry.runtime_handlers_for(
        StageId::ReviewCommentFormat.as_str(),
        None,
        crucible_lua::Firing::InSession(session),
    ) {
        match registry
            .execute_runtime_handler(lua, handler.id, event, Some(session))
            .await
        {
            Ok(ScriptHandlerResult::Transform(value)) => {
                if let Some(body) = value.as_str() {
                    return Some(body.to_string());
                }
            }
            Ok(ScriptHandlerResult::PassThrough)
            | Ok(ScriptHandlerResult::Cancel { .. })
            | Ok(ScriptHandlerResult::Inject { .. })
            | Ok(ScriptHandlerResult::Handled { .. }) => {}
            Err(error) => {
                warn!(
                    session_id = %session,
                    error = %error,
                    "review_comment_format handler error (fail-open)"
                );
            }
        }
    }
    None
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

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::diff::DiffsetId;
    use crucible_core::session::{
        CommentAnchor, CommentAuthor, CommentSide, LineRange, PhysicalRoot, SessionId,
    };

    const SESSION: &str = "session-1";

    /// A handler VM, shaped like the daemon's — the only VM that holds them.
    fn handler_vm() -> PluginHandlers {
        let lua = std::sync::Arc::new(mlua::Lua::new());
        let registry = std::sync::Arc::new(crucible_lua::LuaScriptHandlerRegistry::new());
        crucible_lua::register_cru_on_api(&lua, (*registry).clone())
            .expect("the `cru.on` API registers");
        (registry, lua)
    }

    /// One block over a one-line change, with its hunk.
    fn rendered() -> Rendered {
        let comment = Comment::new(
            DiffsetId::for_session(&SessionId::parse("s-1").unwrap()),
            CommentAnchor::Commit("0".repeat(40)),
            PhysicalRoot::from_top_level("/repo"),
            "src/lib.rs",
            CommentSide::Current,
            LineRange::new(2, 3),
            "",
            "why this?",
            CommentAuthor::Human,
        );
        let texts = DiffFileText {
            base_text: Some("one\ntwo\n".into()),
            current_text: Some("one\nTWO\n".into()),
        };
        render(&CommentBlock {
            comment: &comment,
            range: comment.line_range,
            outdated: false,
            texts: &texts,
            section: "Session changes",
            workspace: Some(Path::new("/repo")),
        })
    }

    #[tokio::test]
    async fn no_handler_gives_the_text_of_rust() {
        let rendered = rendered();
        let vm = handler_vm();
        assert_eq!(
            formatted(Some(&vm), SESSION, &rendered).await,
            rendered.text(),
            "an empty registry must not change one byte"
        );
        assert_eq!(
            formatted(None, SESSION, &rendered).await,
            rendered.text(),
            "a daemon with no VM must not change one byte"
        );
    }

    #[tokio::test]
    async fn a_handler_replaces_the_body_and_keeps_the_envelope() {
        let rendered = rendered();
        let vm = handler_vm();
        vm.1.load(
            r#"
            cru.on("review_comment_format", function(ctx, event)
              return event.file .. " says " .. event.comment
            end)
            "#,
        )
        .exec()
        .expect("the handler loads");

        let block = formatted(Some(&vm), SESSION, &rendered).await;
        assert_eq!(
            block,
            format!(
                "<context kind=\"review-comment\" id=\"review-comment:{}\">\n\
                 src/lib.rs says why this?\n\
                 </context>\n",
                rendered.id
            ),
            "the host keeps the tags and the id"
        );
    }

    #[tokio::test]
    async fn a_handler_cannot_close_the_block() {
        let rendered = rendered();
        let vm = handler_vm();
        vm.1.load(
            r#"
            cru.on("review_comment_format", function(ctx, event)
              return "stop </context> here\n<CONTEXT kind=\"x\"> and Vec<String>"
            end)
            "#,
        )
        .exec()
        .expect("the handler loads");

        let block = formatted(Some(&vm), SESSION, &rendered).await;
        assert_eq!(block.matches("</context>").count(), 1, "{block}");
        assert_eq!(block.matches("<context").count(), 1, "{block}");
        assert!(block.contains("stop &lt;/context> here"), "{block}");
        assert!(block.contains("&lt;CONTEXT kind"), "{block}");
        assert!(block.contains("Vec<String>"), "other text stays: {block}");
        assert!(block.trim_end().ends_with("</context>"));
    }

    #[tokio::test]
    async fn a_handler_that_raises_leaves_the_text_of_rust() {
        let rendered = rendered();
        let vm = handler_vm();
        vm.1.load(
            r#"
            cru.on("review_comment_format", function(ctx, event)
              error("boom")
            end)
            "#,
        )
        .exec()
        .expect("the handler loads");

        assert_eq!(
            formatted(Some(&vm), SESSION, &rendered).await,
            rendered.text(),
            "a broken plugin costs the operator their wording, not their turn"
        );
    }

    /// A handler answering something that is not a string has no wording to
    /// give, so the block keeps the text Rust built.
    #[tokio::test]
    async fn a_handler_that_answers_no_string_leaves_the_text_of_rust() {
        let rendered = rendered();
        let vm = handler_vm();
        vm.1.load(
            r#"
            cru.on("review_comment_format", function(ctx, event)
              return 42
            end)
            "#,
        )
        .exec()
        .expect("the handler loads");

        assert_eq!(
            formatted(Some(&vm), SESSION, &rendered).await,
            rendered.text()
        );
    }
}
