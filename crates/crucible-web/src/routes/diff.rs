//! The diffset routes: a thin proxy over the daemon's `diff.get` and
//! `diff.file` RPCs.
//!
//! The daemon admits the root and reads the files. These routes only turn the
//! query into a `DiffsetSource`. A query names a branch source with `root`, or
//! a session record source with `session`.

use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Query, State},
    Json,
};
use crucible_core::diff::{DiffFileText, Diffset, DiffsetSource};
use crucible_core::session::{PhysicalRoot, SessionId};
use crucible_daemon::rpc_client::DiffFileRequest;
use serde::Deserialize;
use utoipa::IntoParams;
use utoipa_axum::{router::OpenApiRouter, routes};

pub fn diff_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_diff))
        .routes(routes!(get_diff_file))
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
    /// source. Give `root` or `session`, not both.
    root: Option<String>,
    /// The id of a session. Names the record of the session: its session
    /// base, to the files on disk.
    session: Option<String>,
    /// The branch to compare against. Absent takes the default branch.
    base: Option<String>,
    /// The other side. Absent takes the working tree.
    head: Option<String>,
}

impl DiffQuery {
    fn source(self) -> Result<DiffsetSource, WebError> {
        match (self.root, self.session) {
            (Some(root), None) => Ok(branch(root, self.base, self.head)),
            (None, Some(session)) => {
                session_record(&session, self.base.as_deref(), self.head.as_deref())
            }
            (Some(_), Some(_)) | (None, None) => Err(WebError::Validation(
                "give exactly one of root and session".into(),
            )),
        }
    }
}

/// `GET /api/diff` — the files of the branch diff of one root, or of the
/// record of one session.
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
    /// level of the git repository. For a session record, the `root` of the
    /// file entry.
    root: String,
    /// The id of a session. Names the record of the session instead of a
    /// branch.
    session: Option<String>,
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
            base,
            head,
            path,
            from,
        } = self;
        let (source, root) = match session {
            Some(session) => (
                session_record(&session, base.as_deref(), head.as_deref())?,
                Some(PhysicalRoot::from_top_level(root)),
            ),
            // The branch source names its own root.
            None => (branch(root, base, head), None),
        };
        Ok(DiffFileRequest {
            source,
            path,
            from,
            root,
        })
    }
}

/// `GET /api/diff/file` — the two texts of one file of a branch diff or of a
/// session record.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{mock_diff_file_text_for, mock_diffset_for, request_json, shape};

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

    #[tokio::test]
    async fn a_query_without_one_source_is_refused() {
        for uri in [
            "/api/diff",
            "/api/diff?root=/tmp/test-project&session=chat-1",
            "/api/diff?session=chat-1&base=main",
            "/api/diff/file?session=chat-1&root=/tmp/test-project&path=a.md&head=topic",
        ] {
            let (status, _) = request_json("GET", uri, None).await;
            assert_eq!(status, axum::http::StatusCode::UNPROCESSABLE_ENTITY, "{uri}");
        }
    }
}
