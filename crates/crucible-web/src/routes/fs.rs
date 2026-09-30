//! File-tree explorer routes: `GET /api/fs/list` (project dir listing proxy),
//! `POST /api/fs/move` (DnD move / rename), `POST /api/fs/mkdir`, and
//! `POST /api/fs/trash` (context-menu mutations). Live filesystem changes
//! travel on the `system` topic of `GET /api/events` (`routes/events.rs`),
//! not a route of their own.

use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Query, State},
    Json,
};
// The daemon owns every shape these four routes forward. Its module used to
// say so in a comment — "byte-identical to the TypeScript `FsEntry`" — which
// is the kind of contract that cannot fail a build. The routes read the types
// instead.
use crucible_core::protocol::requests::{FsListDirRequest, FsMoveRequest, FsPathRequest};
use crucible_daemon::{FsListing, FsMoveReply, FsTrashReply};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

pub fn fs_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_dir))
        .routes(routes!(move_path))
        .routes(routes!(mkdir_path))
        .routes(routes!(trash_path))
}

/// `GET /api/fs/list` — one directory level inside a registered root.
///
/// All security (registry allowlist, path containment, symlink/dotfile
/// handling) is enforced daemon-side; this handler is a thin passthrough of
/// the daemon's listing envelope.
#[utoipa::path(
    get,
    path = "/api/fs/list",
    params(FsListDirRequest),
    responses(
        (status = 200, body = FsListing),
        (status = 422, description = "The daemon refuses the root or the relative path, and says why"),
        (status = 502, description = "The daemon could not list the directory, or answered a shape this route cannot read"),
    )
)]
async fn list_dir(
    State(state): State<AppState>,
    Query(query): Query<FsListDirRequest>,
) -> Result<Json<FsListing>, WebError> {
    let listing = state
        .daemon
        .fs_list_dir(
            &query.root,
            &query.rel_path,
            query.show_ignored,
            query.show_hidden,
        )
        .await
        .daemon_err()?;

    Ok(Json(listing))
}

/// `POST /api/fs/move` — move or rename one entry within one root.
///
/// The file-tree drag-and-drop backend. All security (allowlist, containment,
/// overwrite refusal) is daemon-side; this handler is a thin passthrough. Kiln
/// note and canvas moves carry the link-rewrite report, so the tree can tell
/// the user what happened to their links.
#[utoipa::path(
    post,
    path = "/api/fs/move",
    request_body = FsMoveRequest,
    responses(
        (status = 200, body = FsMoveReply),
        (status = 422, description = "The daemon refuses the root, either path, or an overwrite, and says why"),
        (status = 502, description = "The daemon could not move the entry, or answered a shape this route cannot read"),
    )
)]
async fn move_path(
    State(state): State<AppState>,
    Json(body): Json<FsMoveRequest>,
) -> Result<Json<FsMoveReply>, WebError> {
    let outcome = state
        .daemon
        .fs_move(&body.root, body.kind, &body.from_rel, &body.to_rel)
        .await
        .daemon_err()?;
    Ok(Json(outcome))
}

/// What `POST /api/fs/mkdir` answers.
///
/// The route's own shape, not the daemon's: `fs_mkdir` reports success as
/// `()`, so there is no daemon body to forward.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct FsMkdirResponse {
    /// Always true. A refusal is an error status, not a `false`.
    created: bool,
}

/// `POST /api/fs/mkdir` — create a folder inside one root.
///
/// The tree's "New folder". Missing parents are created too. Thin daemon proxy.
#[utoipa::path(
    post,
    path = "/api/fs/mkdir",
    request_body = FsPathRequest,
    responses(
        (status = 200, body = FsMkdirResponse),
        (status = 422, description = "The daemon refuses the root or the relative path, and says why"),
        (status = 502, description = "The daemon could not create the folder"),
    )
)]
async fn mkdir_path(
    State(state): State<AppState>,
    Json(body): Json<FsPathRequest>,
) -> Result<Json<FsMkdirResponse>, WebError> {
    state
        .daemon
        .fs_mkdir(&body.root, body.kind, &body.rel_path)
        .await
        .daemon_err()?;
    Ok(Json(FsMkdirResponse { created: true }))
}

/// `POST /api/fs/trash` — move one entry to the root's `.crucible/trash/`.
///
/// The tree's "Delete". Thin daemon proxy; kiln notes leave the index inline,
/// so backlinks re-resolve at once.
#[utoipa::path(
    post,
    path = "/api/fs/trash",
    request_body = FsPathRequest,
    responses(
        (status = 200, body = FsTrashReply),
        (status = 422, description = "The daemon refuses the root or the relative path, and says why"),
        (status = 502, description = "The daemon could not trash the entry, or answered a shape this route cannot read"),
    )
)]
async fn trash_path(
    State(state): State<AppState>,
    Json(body): Json<FsPathRequest>,
) -> Result<Json<FsTrashReply>, WebError> {
    let outcome = state
        .daemon
        .fs_trash(&body.root, body.kind, &body.rel_path)
        .await
        .daemon_err()?;
    Ok(Json(outcome))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        mock_fs_listing, mock_fs_move_reply, mock_fs_trash_reply, request_json, shape, survives,
    };
    use crucible_core::protocol::requests::FsRootKind;
    use crucible_daemon::{SkipReason, SkippedRef};
    use serde_json::json;

    /// The fixture, as the daemon put it on the wire.
    fn sent<T: serde::Serialize>(value: T) -> serde_json::Value {
        serde_json::to_value(value).expect("the daemon's type writes JSON")
    }

    /// The listing reaches the browser as the daemon wrote it.
    ///
    /// A round trip, not a status check: the mock answers with the daemon's
    /// own `FsListing`, the route reads it and writes it again, and the body
    /// has to be the same object. A route that dropped `truncated`, or a row
    /// that dropped `modified`, fails here.
    #[tokio::test]
    async fn list_dir_answers_the_declared_shape() {
        let listing = mock_fs_listing();
        let answered: FsListing = shape("GET", "/api/fs/list?root=/tmp/proj", None).await;

        assert_eq!(sent(&answered), sent(&listing));
        assert_eq!(answered.entries.len(), 2);
        assert!(answered.entries[0].is_dir);
        assert!(!answered.truncated);
    }

    /// A row keeps the two keys a reader could mistake for absent.
    ///
    /// `modified` is null for a platform that cannot report it and `status` is
    /// null until the git decoration lands. Both are WRITTEN, so a client
    /// reads null rather than `undefined`.
    #[tokio::test]
    async fn a_listing_row_writes_its_null_fields() {
        let answered: serde_json::Value = shape("GET", "/api/fs/list?root=/tmp/proj", None).await;

        let row = &answered["entries"][1];
        assert!(row.get("modified").is_some(), "modified is absent: {row}");
        assert_eq!(row["modified"], json!(null));
        assert!(row.get("status").is_some(), "status is absent: {row}");
        assert_eq!(row["status"], json!(null));
    }

    /// The link-rewrite report reaches the browser as the daemon wrote it.
    ///
    /// The second round trip. `rewritten_sources` and `skipped` are what the
    /// file tree tells the user about their links, and a named reply is the
    /// first thing that could drop them.
    #[tokio::test]
    async fn move_path_answers_the_declared_shape() {
        let reply = mock_fs_move_reply();
        let answered: FsMoveReply = shape(
            "POST",
            "/api/fs/move",
            Some(json!({
                "root": "/tmp/k",
                "kind": "kiln",
                "from_rel": "a.md",
                "to_rel": "notes/a.md",
            })),
        )
        .await;

        assert_eq!(sent(&answered), sent(&reply));
        assert!(answered.moved);
        assert_eq!(
            answered.skipped.as_ref().expect("a skipped list")[0].reason,
            SkipReason::Ambiguous
        );
    }

    /// Both arms of the move reply survive the round trip, each as itself.
    ///
    /// A move the link index does not watch answers `moved` ALONE. Writing the
    /// two report keys as null instead would tell the browser that a directory
    /// move rewrote no links, where today it says nothing about links at all.
    #[test]
    fn a_move_reply_omits_the_link_report_it_has_nothing_to_say_about() {
        let plain = FsMoveReply {
            moved: true,
            rewritten_sources: None,
            skipped: None,
        };
        assert_eq!(sent(&plain), json!({ "moved": true }));
        survives::<FsMoveReply>(&plain);

        let reported = FsMoveReply {
            moved: true,
            rewritten_sources: Some(vec!["index.md".to_string()]),
            skipped: Some(vec![SkippedRef {
                source_path: "other.md".to_string(),
                raw_target: "a".to_string(),
                reason: SkipReason::StaleSpan,
            }]),
        };
        assert_eq!(
            sent(&reported),
            json!({
                "moved": true,
                "rewritten_sources": ["index.md"],
                "skipped": [{
                    "source_path": "other.md",
                    "raw_target": "a",
                    "reason": "stale-span",
                }],
            })
        );
        survives::<FsMoveReply>(&reported);
    }

    #[tokio::test]
    async fn mkdir_path_answers_the_declared_shape() {
        let answered: FsMkdirResponse = shape(
            "POST",
            "/api/fs/mkdir",
            Some(json!({ "root": "/tmp/proj", "kind": "project", "rel_path": "new/dir" })),
        )
        .await;

        assert!(answered.created);
    }

    /// The trash reply reaches the browser as the daemon wrote it.
    #[tokio::test]
    async fn trash_path_answers_the_declared_shape() {
        let reply = mock_fs_trash_reply();
        let answered: FsTrashReply = shape(
            "POST",
            "/api/fs/trash",
            Some(json!({ "root": "/tmp/k", "kind": "kiln", "rel_path": "a.md" })),
        )
        .await;

        assert_eq!(sent(&answered), sent(&reply));
        assert!(answered.trashed);
        assert!(answered.trash_path.starts_with(".crucible/trash/"));
    }

    /// An unknown kind is refused before a handler runs.
    ///
    /// `kind` is `FsRootKind`, not a `String` (Simplification Plan step 18):
    /// a body naming a third kind now fails `axum`'s own `Json<T>` extraction
    /// with 422, the same status this route answers for every other bad
    /// body. It used to reach the mock daemon as a plain string and read as
    /// a 200 — see `git log -p` on this test for the earlier behavior.
    #[tokio::test]
    async fn an_unknown_root_kind_is_refused_before_the_daemon() {
        let (status, _) = request_json(
            "POST",
            "/api/fs/mkdir",
            Some(json!({ "root": "/tmp/proj", "kind": "folder", "rel_path": "x" })),
        )
        .await;

        assert_eq!(status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
    }

    /// `FsRootKind`'s wire spelling, as one exhaustive table.
    ///
    /// A third kind fails to compile here until somebody decides what the
    /// daemon calls it.
    fn root_kind_on_the_wire(kind: FsRootKind) -> &'static str {
        match kind {
            FsRootKind::Project => "project",
            FsRootKind::Kiln => "kiln",
        }
    }

    /// The document's `kind` values are the ones `resolve_root` admits.
    ///
    /// `crucible-daemon`'s `server::fs::resolve_root` matches these two
    /// strings and refuses every other. The document says so, so the generated
    /// client offers a union rather than an open string.
    #[test]
    fn every_root_kind_is_one_the_daemon_admits() {
        use utoipa::PartialSchema;

        let table = [FsRootKind::Project, FsRootKind::Kiln];
        for kind in table {
            assert_eq!(
                serde_json::to_value(kind).expect("a kind writes JSON"),
                json!(root_kind_on_the_wire(kind)),
                "the serde spelling left the table behind"
            );
        }

        let schema = sent(FsRootKind::schema());
        let declared: Vec<&str> = schema["enum"]
            .as_array()
            .unwrap_or_else(|| panic!("`FsRootKind` is not an enum schema: {schema:#}"))
            .iter()
            .map(|value| value.as_str().expect("an enum value is a string"))
            .collect();
        let expected: Vec<&str> = table.into_iter().map(root_kind_on_the_wire).collect();
        assert_eq!(declared, expected, "the document names other kinds");
    }

    /// `SkipReason`'s wire spelling, as one exhaustive table.
    ///
    /// A fifth reason fails to compile here. The browser prints a sentence per
    /// reason, so a reason it has no arm for is a blank line in the file tree.
    fn skip_reason_on_the_wire(reason: SkipReason) -> &'static str {
        match reason {
            SkipReason::Ambiguous => "ambiguous",
            SkipReason::StaleSpan => "stale-span",
            SkipReason::CanvasNoExactMatch => "canvas-no-exact-match",
            SkipReason::CanvasUnreadable => "canvas-unreadable",
        }
    }

    /// Every skip reason keeps the spelling it had, and the document names all
    /// four.
    #[test]
    fn every_skip_reason_reaches_the_wire() {
        use utoipa::PartialSchema;

        let table = [
            SkipReason::Ambiguous,
            SkipReason::StaleSpan,
            SkipReason::CanvasNoExactMatch,
            SkipReason::CanvasUnreadable,
        ];
        for reason in table {
            assert_eq!(
                serde_json::to_value(reason).expect("a reason writes JSON"),
                json!(skip_reason_on_the_wire(reason)),
                "the serde spelling left the table behind"
            );
        }

        let schema = sent(SkipReason::schema());
        let declared: Vec<&str> = schema["enum"]
            .as_array()
            .unwrap_or_else(|| panic!("`SkipReason` is not an enum schema: {schema:#}"))
            .iter()
            .map(|value| value.as_str().expect("an enum value is a string"))
            .collect();
        let expected: Vec<&str> = table.into_iter().map(skip_reason_on_the_wire).collect();
        assert_eq!(declared, expected, "the document names other reasons");
    }
}
