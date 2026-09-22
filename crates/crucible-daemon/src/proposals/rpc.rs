//! The `proposal.*` RPCs. Each handler reads its params, calls the store and
//! answers the proposal.

use crate::protocol::{Request, Response, INTERNAL_ERROR, INVALID_PARAMS};
use crate::rpc_client::{
    ProposalIdRequest, ProposalListRequest, ProposalRejectRequest, ProposalResolveRequest,
};

use super::{ProposalError, ProposalResult, ProposalStore};

/// The answer for a store result.
///
/// An unknown id, a settled proposal and an operation that the daemon does
/// not serve yet are the caller's to fix. A store failure is the daemon's.
fn answer<T: serde::Serialize>(req: &Request, result: ProposalResult<T>) -> Response {
    let id = req.id.clone();
    match result
        .and_then(|value| serde_json::to_value(value).map_err(|e| ProposalError::Store(e.into())))
    {
        Ok(value) => Response::success(id, value),
        Err(
            e @ (ProposalError::NotFound(_)
            | ProposalError::Settled(..)
            | ProposalError::NotServed(_)),
        ) => Response::error(id, INVALID_PARAMS, e.to_string()),
        Err(ProposalError::Store(e)) => Response::error(id, INTERNAL_ERROR, format!("{e:#}")),
    }
}

macro_rules! params {
    ($req:expr, $ty:ty) => {
        match crate::rpc_helpers::typed_params::<$ty>(&$req) {
            Ok(p) => p,
            Err(response) => return *response,
        }
    };
}

/// Handle `proposal.list`.
pub(crate) async fn handle_proposal_list(req: Request, store: &ProposalStore) -> Response {
    let params = params!(req, ProposalListRequest);
    answer(&req, store.list(params.all))
}

/// Handle `proposal.get`.
pub(crate) async fn handle_proposal_get(req: Request, store: &ProposalStore) -> Response {
    let params = params!(req, ProposalIdRequest);
    answer(&req, store.get(&params.id))
}

/// Handle `proposal.accept`.
pub(crate) async fn handle_proposal_accept(req: Request, store: &ProposalStore) -> Response {
    let params = params!(req, ProposalIdRequest);
    answer(&req, store.accept(&params.id))
}

/// Handle `proposal.reject`.
pub(crate) async fn handle_proposal_reject(req: Request, store: &ProposalStore) -> Response {
    let params = params!(req, ProposalRejectRequest);
    answer(&req, store.reject(&params.id, params.reason))
}

/// Handle `proposal.dismiss`.
pub(crate) async fn handle_proposal_dismiss(req: Request, store: &ProposalStore) -> Response {
    let params = params!(req, ProposalIdRequest);
    answer(&req, store.dismiss(&params.id))
}

/// Handle `proposal.resolve`.
pub(crate) async fn handle_proposal_resolve(req: Request, store: &ProposalStore) -> Response {
    let params = params!(req, ProposalResolveRequest);
    answer(&req, store.resolve(&params.id, &params.path, &params.text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::RequestId;
    use crucible_core::file_write::ExpectedBase;
    use crucible_core::proposal::{Proposal, ProposalAuthor, ProposalId, ProposalState};
    use crucible_core::session::{PhysicalRoot, SessionId};
    use serde_json::{json, Value};

    fn request(method: &str, params: Value) -> Request {
        Request {
            jsonrpc: "2.0".to_string(),
            id: Some(RequestId::Number(1)),
            method: method.to_string(),
            params,
        }
    }

    fn made(store: &ProposalStore) -> Proposal {
        store
            .record_write(
                ProposalAuthor::Plugin {
                    name: "reflection".into(),
                },
                &SessionId::parse("aux-1").unwrap(),
                PhysicalRoot::from_top_level("/kiln"),
                "a.md",
                ExpectedBase::Absent,
                "text".into(),
            )
            .unwrap()
    }

    #[tokio::test]
    async fn the_proposal_rpcs_answer_the_proposal() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = ProposalStore::new(dir.path().join("proposals"));
        let proposal = made(&store);
        let id = json!({ "id": proposal.id });

        let listed = handle_proposal_list(request("proposal.list", json!({})), &store).await;
        let listed: Vec<Proposal> = serde_json::from_value(listed.result.unwrap()).unwrap();
        assert_eq!(listed, vec![proposal.clone()]);

        let got = handle_proposal_get(request("proposal.get", id.clone()), &store).await;
        assert_eq!(
            serde_json::from_value::<Proposal>(got.result.unwrap()).unwrap(),
            proposal
        );

        let rejected = handle_proposal_reject(
            request(
                "proposal.reject",
                json!({ "id": proposal.id, "reason": "no" }),
            ),
            &store,
        )
        .await;
        let rejected: Proposal = serde_json::from_value(rejected.result.unwrap()).unwrap();
        assert_eq!(
            rejected.state,
            ProposalState::Rejected {
                reason: Some("no".into())
            }
        );

        // A settled proposal, an unknown id and a stub are the caller's error.
        let again = handle_proposal_dismiss(request("proposal.dismiss", id.clone()), &store).await;
        assert_eq!(again.error.unwrap().code, INVALID_PARAMS);
        let unknown = handle_proposal_get(
            request("proposal.get", json!({ "id": ProposalId::generate() })),
            &store,
        )
        .await;
        assert_eq!(unknown.error.unwrap().code, INVALID_PARAMS);
        let accept = handle_proposal_accept(request("proposal.accept", id), &store).await;
        assert_eq!(accept.error.unwrap().code, INVALID_PARAMS);
        let bad =
            handle_proposal_get(request("proposal.get", json!({ "id": "../x" })), &store).await;
        assert_eq!(bad.error.unwrap().code, INVALID_PARAMS);
    }
}
