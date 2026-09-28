//! Proposal RPC methods: list, get, accept, reject, dismiss and resolve.
//!
//! A read retries. A decision is sent once, because a retry after a timeout
//! can repeat a decision that the daemon already made.

use anyhow::Result;
use crucible_core::proposal::{Proposal, ProposalFile, ProposalId};
use crucible_core::protocol::requests::*;
use crucible_core::protocol::RpcMethod;

use super::DaemonClient;

impl DaemonClient {
    /// `proposal.list`: the proposals in the Inbox, oldest first. With
    /// `all`, every stored proposal.
    pub async fn proposal_list(&self, all: bool) -> Result<Vec<Proposal>> {
        self.typed_call_with_retry(RpcMethod::ProposalList, ProposalListRequest { all })
            .await
    }

    /// `proposal.get`: one proposal.
    pub async fn proposal_get(&self, id: &ProposalId) -> Result<Proposal> {
        self.typed_call_with_retry(RpcMethod::ProposalGet, ProposalIdRequest { id: *id })
            .await
    }

    /// `proposal.accept`: write every file of the proposal.
    pub async fn proposal_accept(&self, id: &ProposalId) -> Result<Proposal> {
        self.proposal_accept_paths(id, &[]).await
    }

    /// `proposal.accept` of the files `paths`. The answer is the proposal
    /// that holds those files. Empty `paths` accepts every file.
    pub async fn proposal_accept_paths(
        &self,
        id: &ProposalId,
        paths: &[String],
    ) -> Result<Proposal> {
        self.proposal_accept_files(id, paths, &[]).await
    }

    /// Accept qualified files, or unique legacy paths.
    pub async fn proposal_accept_files(
        &self,
        id: &ProposalId,
        paths: &[String],
        files: &[ProposalFile],
    ) -> Result<Proposal> {
        self.typed_call(
            RpcMethod::ProposalAccept,
            ProposalAcceptRequest {
                id: *id,
                paths: paths.to_vec(),
                files: files.to_vec(),
            },
        )
        .await
    }

    /// `proposal.reject`: reject the proposal, with an optional reason.
    pub async fn proposal_reject(&self, id: &ProposalId, reason: Option<&str>) -> Result<Proposal> {
        self.proposal_reject_paths(id, &[], reason).await
    }

    /// `proposal.reject` of the files `paths`. The answer is the proposal
    /// that holds those files. Empty `paths` rejects every file.
    pub async fn proposal_reject_paths(
        &self,
        id: &ProposalId,
        paths: &[String],
        reason: Option<&str>,
    ) -> Result<Proposal> {
        self.proposal_reject_files(id, paths, &[], reason).await
    }

    /// Reject qualified files, or unique legacy paths.
    pub async fn proposal_reject_files(
        &self,
        id: &ProposalId,
        paths: &[String],
        files: &[ProposalFile],
        reason: Option<&str>,
    ) -> Result<Proposal> {
        self.typed_call(
            RpcMethod::ProposalReject,
            ProposalRejectRequest {
                id: *id,
                reason: reason.map(str::to_string),
                paths: paths.to_vec(),
                files: files.to_vec(),
            },
        )
        .await
    }

    /// `proposal.dismiss`: take the proposal out of the Inbox with no
    /// decision.
    pub async fn proposal_dismiss(&self, id: &ProposalId) -> Result<Proposal> {
        self.typed_call(RpcMethod::ProposalDismiss, ProposalIdRequest { id: *id })
            .await
    }

    /// `proposal.resolve`: write the settled text of one conflicted file.
    pub async fn proposal_resolve(
        &self,
        id: &ProposalId,
        path: &str,
        text: &str,
    ) -> Result<Proposal> {
        self.proposal_resolve_file(id, path, None, text).await
    }

    /// Resolve exactly one file of a proposal.
    pub async fn proposal_resolve_file(
        &self,
        id: &ProposalId,
        path: &str,
        root: Option<&crucible_core::session::PhysicalRoot>,
        text: &str,
    ) -> Result<Proposal> {
        self.typed_call(
            RpcMethod::ProposalResolve,
            ProposalResolveRequest {
                id: *id,
                path: path.to_string(),
                root: root.cloned(),
                text: text.to_string(),
            },
        )
        .await
    }
}
