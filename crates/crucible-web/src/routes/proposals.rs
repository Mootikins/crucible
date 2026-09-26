//! The proposal routes: a thin proxy over the daemon's `proposal.*` RPCs.
//!
//! A proposal belongs to no session, so the routes live under
//! `/api/proposals`, not under a session. The daemon owns every decision.
//!
//! Each decision route is a POST with a JSON body. The browser sends `{}`
//! when it has nothing to say, because the body forces
//! `Content-Type: application/json`, and so the CORS preflight.

use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use crucible_core::proposal::{Proposal, ProposalFile, ProposalId};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

pub fn proposal_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_proposals))
        .routes(routes!(get_proposal))
        .routes(routes!(accept_proposal))
        .routes(routes!(reject_proposal))
        .routes(routes!(dismiss_proposal))
        .routes(routes!(resolve_proposal))
}

/// Read a proposal id from the path. A malformed id is the caller's error.
fn proposal_id(raw: &str) -> Result<ProposalId, WebError> {
    raw.parse()
        .map_err(|_| WebError::Validation(format!("not a proposal id: {raw:?}")))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct ListQuery {
    /// Also list the proposals that left the Inbox: accepted, rejected and
    /// dismissed.
    #[serde(default)]
    all: bool,
}

/// `GET /api/proposals` — the proposals in the Inbox, oldest first.
#[utoipa::path(
    get,
    path = "/api/proposals",
    params(ListQuery),
    responses(
        (status = 200, body = Vec<Proposal>),
        (status = 502, description = "The daemon could not read the proposals"),
    )
)]
async fn list_proposals(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<Proposal>>, WebError> {
    let proposals = state.daemon.proposal_list(query.all).await.daemon_err()?;
    Ok(Json(proposals))
}

/// `GET /api/proposals/{id}` — one proposal, in any state.
#[utoipa::path(
    get,
    path = "/api/proposals/{id}",
    params(("id" = String, Path, description = "The proposal")),
    responses(
        (status = 200, body = Proposal),
        (status = 422, description = "No proposal has the id"),
        (status = 502, description = "The daemon could not read the proposal"),
    )
)]
async fn get_proposal(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Proposal>, WebError> {
    let id = proposal_id(&id)?;
    let proposal = state.daemon.proposal_get(&id).await.daemon_err()?;
    Ok(Json(proposal))
}

/// The body of an accept.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct AcceptProposalBody {
    /// The files to write, as the proposal names them. The daemon moves them
    /// into a new proposal and accepts that one. Absent or empty means every
    /// file.
    #[serde(default)]
    pub paths: Vec<String>,
    /// Root-qualified files. Use this instead of paths for per-file decisions.
    #[serde(default)]
    pub files: Vec<ProposalFile>,
}

/// `POST /api/proposals/{id}/accept` — write every file of the proposal, or
/// the files that the body names.
///
/// The reply is the proposal that holds the written files.
#[utoipa::path(
    post,
    path = "/api/proposals/{id}/accept",
    params(("id" = String, Path, description = "The proposal")),
    request_body = AcceptProposalBody,
    responses(
        (status = 200, body = Proposal),
        (status = 422, description = "No proposal has the id, or the proposal is already settled"),
        (status = 502, description = "The daemon could not write the files"),
    )
)]
async fn accept_proposal(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<AcceptProposalBody>,
) -> Result<Json<Proposal>, WebError> {
    let id = proposal_id(&id)?;
    let proposal = state
        .daemon
        .proposal_accept_files(&id, &body.paths, &body.files)
        .await
        .daemon_err()?;
    Ok(Json(proposal))
}

/// The body of a reject.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct RejectProposalBody {
    /// Why the user rejects the proposal. The proposal keeps it.
    #[serde(default)]
    pub reason: Option<String>,
    /// The files to reject, as the proposal names them. The daemon moves
    /// them into a new proposal and rejects that one. Absent or empty means
    /// every file.
    #[serde(default)]
    pub paths: Vec<String>,
    /// Root-qualified files. Use this instead of paths for per-file decisions.
    #[serde(default)]
    pub files: Vec<ProposalFile>,
}

/// `POST /api/proposals/{id}/reject` — reject the proposal, or the files that
/// the body names. No file changes.
///
/// The reply is the proposal that holds the rejected files.
#[utoipa::path(
    post,
    path = "/api/proposals/{id}/reject",
    params(("id" = String, Path, description = "The proposal")),
    request_body = RejectProposalBody,
    responses(
        (status = 200, body = Proposal),
        (status = 422, description = "No proposal has the id, or the proposal is already settled"),
        (status = 502, description = "The daemon could not store the decision"),
    )
)]
async fn reject_proposal(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<RejectProposalBody>,
) -> Result<Json<Proposal>, WebError> {
    let id = proposal_id(&id)?;
    let proposal = state
        .daemon
        .proposal_reject_files(&id, &body.paths, &body.files, body.reason.as_deref())
        .await
        .daemon_err()?;
    Ok(Json(proposal))
}

/// `POST /api/proposals/{id}/dismiss` — take the proposal out of the Inbox
/// with no decision. The daemon keeps its file.
#[utoipa::path(
    post,
    path = "/api/proposals/{id}/dismiss",
    params(("id" = String, Path, description = "The proposal")),
    responses(
        (status = 200, body = Proposal),
        (status = 422, description = "No proposal has the id, or the proposal is already settled"),
        (status = 502, description = "The daemon could not store the decision"),
    )
)]
async fn dismiss_proposal(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Proposal>, WebError> {
    let id = proposal_id(&id)?;
    let proposal = state.daemon.proposal_dismiss(&id).await.daemon_err()?;
    Ok(Json(proposal))
}

/// The body of a resolve: the text that the user settled for one file.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ResolveProposalBody {
    /// The path relative to the kiln root, as the proposal names it.
    pub path: String,
    /// The stored kiln root. Omit only when the path is unique in the proposal.
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    pub root: Option<crucible_core::session::PhysicalRoot>,
    /// The whole text to write.
    pub text: String,
}

/// `POST /api/proposals/{id}/resolve` — write the settled text of one
/// conflicted file.
#[utoipa::path(
    post,
    path = "/api/proposals/{id}/resolve",
    params(("id" = String, Path, description = "The proposal")),
    request_body = ResolveProposalBody,
    responses(
        (status = 200, body = Proposal),
        (status = 422, description = "No proposal has the id, or the file has no conflict"),
        (status = 502, description = "The daemon could not write the file"),
    )
)]
async fn resolve_proposal(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ResolveProposalBody>,
) -> Result<Json<Proposal>, WebError> {
    let id = proposal_id(&id)?;
    let proposal = state
        .daemon
        .proposal_resolve_file(&id, &body.path, body.root.as_ref(), &body.text)
        .await
        .daemon_err()?;
    Ok(Json(proposal))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{mock_proposal_for, mock_proposal_id, request_json, shape};
    use crucible_core::proposal::ProposalState;
    use serde_json::json;

    fn id() -> ProposalId {
        mock_proposal_id()
    }

    /// Each route reaches the daemon with the id of its path, and the reply
    /// reaches the browser as the daemon wrote it.
    #[tokio::test]
    async fn the_proposal_routes_answer_the_declared_shape() {
        let id_text = id().to_string();
        let listed: Vec<Proposal> = shape("GET", "/api/proposals", None).await;
        assert_eq!(listed, vec![mock_proposal_for(id(), ProposalState::Open)]);
        let all: Vec<Proposal> = shape("GET", "/api/proposals?all=true", None).await;
        assert_eq!(all.len(), 2);

        let got: Proposal = shape("GET", &format!("/api/proposals/{id_text}"), None).await;
        assert_eq!(got, mock_proposal_for(id(), ProposalState::Open));

        let accepted: Proposal = shape(
            "POST",
            &format!("/api/proposals/{id_text}/accept"),
            Some(json!({})),
        )
        .await;
        assert_eq!(accepted, mock_proposal_for(id(), ProposalState::Accepted));
        // A body with paths decides only those files.
        let one: Proposal = shape(
            "POST",
            &format!("/api/proposals/{id_text}/accept"),
            Some(json!({ "paths": ["notes/a.md"] })),
        )
        .await;
        assert_eq!(one.title, "Change notes/a.md");
        let one: Proposal = shape(
            "POST",
            &format!("/api/proposals/{id_text}/reject"),
            Some(json!({ "paths": ["notes/b.md", "notes/c.md"] })),
        )
        .await;
        assert_eq!(one.title, "Change notes/b.md, notes/c.md");

        let rejected: Proposal = shape(
            "POST",
            &format!("/api/proposals/{id_text}/reject"),
            Some(json!({ "reason": "not now" })),
        )
        .await;
        assert_eq!(
            rejected.state,
            ProposalState::Rejected {
                reason: Some("not now".into())
            }
        );

        let dismissed: Proposal = shape(
            "POST",
            &format!("/api/proposals/{id_text}/dismiss"),
            Some(json!({})),
        )
        .await;
        assert_eq!(dismissed.state, ProposalState::Dismissed);

        let resolved: Proposal = shape(
            "POST",
            &format!("/api/proposals/{id_text}/resolve"),
            Some(json!({ "path": "a.md", "text": "settled" })),
        )
        .await;
        assert_eq!(resolved, mock_proposal_for(id(), ProposalState::Accepted));
    }

    #[tokio::test]
    async fn a_malformed_proposal_id_is_refused() {
        let (status, _) = request_json("GET", "/api/proposals/not-a-uuid", None).await;
        assert_eq!(status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
    }
}
