//! The `proposal.*` RPCs. Each handler reads its params, calls the store and
//! answers the proposal.

use std::path::PathBuf;

use crucible_core::proposal::ProposalId;
use crucible_core::session::PhysicalRoot;

use crate::kiln_manager::KilnManager;
use crate::protocol::{Request, Response, INTERNAL_ERROR, INVALID_PARAMS};
use crate::rpc_client::{
    ProposalAcceptRequest, ProposalIdRequest, ProposalListRequest, ProposalRejectRequest,
    ProposalResolveRequest,
};

use super::{ProposalError, ProposalResult, ProposalStore};

/// The answer for a store result.
///
/// An unknown id, a settled proposal, a file that the proposal does not write
/// and a resolve of a file with no conflict are the caller's to fix. A store failure and a refused write are the
/// daemon's.
fn answer<T: serde::Serialize>(req: &Request, result: ProposalResult<T>) -> Response {
    let id = req.id.clone();
    match result
        .and_then(|value| serde_json::to_value(value).map_err(|e| ProposalError::Store(e.into())))
    {
        Ok(value) => Response::success(id, value),
        Err(
            e @ (ProposalError::NotFound(_)
            | ProposalError::Settled(..)
            | ProposalError::NoWrite(..)
            | ProposalError::NoConflict(..)
            | ProposalError::Ambiguous(..)
            | ProposalError::MixedSelection),
        ) => Response::error(id, INVALID_PARAMS, e.to_string()),
        Err(e @ ProposalError::Busy(_)) => Response::error(id, -32009, e.to_string()),
        Err(e @ ProposalError::WriteFailed(_)) => {
            Response::error(id, INTERNAL_ERROR, e.to_string())
        }
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

/// Handle `proposal.list`. The daemon does not watch a closed kiln, so the
/// list runs the stale check first.
pub(crate) async fn handle_proposal_list(req: Request, store: &ProposalStore) -> Response {
    let params = params!(req, ProposalListRequest);
    answer(
        &req,
        store.check_stale().and_then(|_| store.list(params.all)),
    )
}

/// The kiln roots that the daemon admits for a write. The daemon also opens
/// each kiln that `id` writes, so the watcher and the index see the write.
async fn write_roots(store: &ProposalStore, id: &ProposalId, km: &KilnManager) -> Vec<PathBuf> {
    if let Ok(proposal) = store.get(id) {
        let mut roots: Vec<&PhysicalRoot> = proposal.writes.iter().map(|w| &w.root).collect();
        roots.dedup();
        for root in roots {
            // A refusal here shows again as a refused write, with its path.
            let _opened = km.admit_kiln_root(root.as_path()).await;
        }
    }
    km.admissible_kiln_roots().await
}

/// Handle `proposal.get`.
pub(crate) async fn handle_proposal_get(req: Request, store: &ProposalStore) -> Response {
    let params = params!(req, ProposalIdRequest);
    answer(&req, store.get(&params.id))
}

/// Handle `proposal.accept`. With `paths`, only those files.
pub(crate) async fn handle_proposal_accept(
    req: Request,
    store: &ProposalStore,
    km: &KilnManager,
) -> Response {
    let params = params!(req, ProposalAcceptRequest);
    let kilns = write_roots(store, &params.id, km).await;
    answer(
        &req,
        store
            .accept_files(&params.id, &params.paths, &params.files, &kilns)
            .await,
    )
}

/// Handle `proposal.reject`. With `paths`, only those files.
pub(crate) async fn handle_proposal_reject(req: Request, store: &ProposalStore) -> Response {
    let params = params!(req, ProposalRejectRequest);
    answer(
        &req,
        store.reject_files(&params.id, &params.paths, &params.files, params.reason),
    )
}

/// Handle `proposal.dismiss`.
pub(crate) async fn handle_proposal_dismiss(req: Request, store: &ProposalStore) -> Response {
    let params = params!(req, ProposalIdRequest);
    answer(&req, store.dismiss(&params.id))
}

/// Handle `proposal.resolve`.
pub(crate) async fn handle_proposal_resolve(
    req: Request,
    store: &ProposalStore,
    km: &KilnManager,
) -> Response {
    let params = params!(req, ProposalResolveRequest);
    let kilns = write_roots(store, &params.id, km).await;
    answer(
        &req,
        store
            .resolve_file(
                &params.id,
                &params.path,
                params.root.as_ref(),
                &params.text,
                &kilns,
            )
            .await,
    )
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

        // A settled proposal and an unknown id are the caller's error.
        let again = handle_proposal_dismiss(request("proposal.dismiss", id), &store).await;
        assert_eq!(again.error.unwrap().code, INVALID_PARAMS);
        let unknown = handle_proposal_get(
            request("proposal.get", json!({ "id": ProposalId::generate() })),
            &store,
        )
        .await;
        assert_eq!(unknown.error.unwrap().code, INVALID_PARAMS);
        let bad =
            handle_proposal_get(request("proposal.get", json!({ "id": "../x" })), &store).await;
        assert_eq!(bad.error.unwrap().code, INVALID_PARAMS);
    }
}
