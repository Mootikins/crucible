//! The diffset routes: a thin proxy over the daemon's `diff.get` and
//! `diff.file` RPCs.
//!
//! The daemon admits the root and reads git. These routes only turn the query
//! into a `DiffsetSource`. Today they name the branch source only.

use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Query, State},
    Json,
};
use crucible_core::diff::{DiffFileText, Diffset, DiffsetSource};
use crucible_core::session::PhysicalRoot;
use serde::Deserialize;
use utoipa::IntoParams;
use utoipa_axum::{router::OpenApiRouter, routes};

pub fn diff_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_diff))
        .routes(routes!(get_diff_file))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct BranchQuery {
    /// Absolute path of the top level of a git repository.
    root: String,
    /// The branch to compare against. Absent takes the default branch.
    base: Option<String>,
    /// The other side. Absent takes the working tree.
    head: Option<String>,
}

impl BranchQuery {
    fn source(self) -> DiffsetSource {
        DiffsetSource::Branch {
            // The daemon resolves the top level and refuses any other path.
            root: PhysicalRoot::from_top_level(self.root),
            base: self.base.unwrap_or_default(),
            head: self.head,
        }
    }
}

/// `GET /api/diff` — the files of the branch diff of one root.
///
/// The reply has the counts of each file and no text. Its `source` names the
/// base branch that the daemon used.
#[utoipa::path(
    get,
    path = "/api/diff",
    params(BranchQuery),
    responses(
        (status = 200, body = Diffset),
        (status = 422, description = "The daemon refuses the root or the branch, and says why"),
        (status = 502, description = "git failed, or the daemon could not be reached"),
    )
)]
async fn get_diff(
    State(state): State<AppState>,
    Query(query): Query<BranchQuery>,
) -> Result<Json<Diffset>, WebError> {
    let diffset = state.daemon.diff_get(&query.source()).await.daemon_err()?;
    Ok(Json(diffset))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct FileQuery {
    /// Absolute path of the top level of a git repository.
    root: String,
    /// The branch to compare against. Absent takes the default branch.
    base: Option<String>,
    /// The other side. Absent takes the working tree.
    head: Option<String>,
    /// The path of the file relative to `root`, on the current side.
    path: String,
    /// The old path of a renamed file.
    from: Option<String>,
}

/// `GET /api/diff/file` — the two texts of one file of a branch diff.
///
/// A side is `null` when the file is absent on it, binary or too large.
#[utoipa::path(
    get,
    path = "/api/diff/file",
    params(FileQuery),
    responses(
        (status = 200, body = DiffFileText),
        (status = 422, description = "The daemon refuses the root, the branch or the path, and says why"),
        (status = 502, description = "git failed, or the daemon could not be reached"),
    )
)]
async fn get_diff_file(
    State(state): State<AppState>,
    Query(query): Query<FileQuery>,
) -> Result<Json<DiffFileText>, WebError> {
    let FileQuery {
        root,
        base,
        head,
        path,
        from,
    } = query;
    let source = BranchQuery { root, base, head }.source();
    let text = state
        .daemon
        .diff_file(&source, &path, from.as_deref())
        .await
        .daemon_err()?;
    Ok(Json(text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{mock_diff_file_text_for, mock_diffset_for, shape};
    use crucible_daemon::rpc_client::DiffFileRequest;

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
        };
        assert_eq!(text, mock_diff_file_text_for(&request));
    }
}
