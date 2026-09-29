//! Repository cloning — a thin proxy over the daemon's `scm.clone` RPC.
//!
//! Branch listing and worktree creation used to live here too. They are the
//! worktree plugin's business now: this layer held a second copy of what a
//! branch is, and the daemon held a third.

use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{extract::State, Json};
// The daemon owns the `scm.clone` reply, so this route answers its type
// rather than a second copy of it.
use crucible_core::protocol::requests::ScmCloneRequest;
use crucible_daemon::ScmCloneResponse;
use std::path::Path;
use utoipa_axum::{router::OpenApiRouter, routes};

pub fn scm_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(clone_repo))
}

/// `POST /api/scm/clone` — clone a remote repo and register it as a project.
///
/// It lands in `[workspace] root_dir` unless `dest` says otherwise. Slow by
/// nature: the daemon call carries a ten-minute timeout, and this route does
/// not retry, because a second `git clone` over a partial directory refuses
/// with a message about the destination rather than the network.
#[utoipa::path(
    post,
    path = "/api/scm/clone",
    request_body = ScmCloneRequest,
    responses(
        (status = 200, body = ScmCloneResponse),
        (status = 422, description = "The daemon refuses the URL or the destination, and says why"),
        (status = 502, description = "The clone failed, or the daemon could not be reached"),
    )
)]
async fn clone_repo(
    State(state): State<AppState>,
    Json(req): Json<ScmCloneRequest>,
) -> Result<Json<ScmCloneResponse>, WebError> {
    let result = state
        .daemon
        .scm_clone(
            &req.url,
            req.dest.as_deref().map(Path::new),
            req.name.as_deref(),
        )
        .await
        .daemon_err()?;
    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{mock_scm_clone, shape};
    use serde_json::json;

    /// The clone result reaches the browser as the daemon wrote it.
    ///
    /// A round trip through the daemon's own `ScmCloneResponse`: the mock
    /// answers with that type, the route reads it and writes it again, and the
    /// body has to be the same object. The nested `Project` is the part a
    /// named reply could quietly narrow — the splash registers the clone from
    /// it and then opens it.
    #[tokio::test]
    async fn clone_repo_answers_the_declared_shape() {
        let cloned = mock_scm_clone();
        let answered: ScmCloneResponse = shape(
            "POST",
            "/api/scm/clone",
            Some(json!({ "url": "https://example.invalid/test-project.git" })),
        )
        .await;

        let sent = serde_json::to_value(&cloned).expect("the daemon's type writes JSON");
        assert_eq!(
            serde_json::to_value(&answered).expect("the reply writes JSON"),
            sent
        );
        assert_eq!(answered.path, cloned.path);
        assert_eq!(answered.project.name, cloned.project.name);
    }
}
