//! Proposal RPC methods: list, get, accept, reject, dismiss and resolve.
//!
//! A read retries. A decision is sent once, because a retry after a timeout
//! can repeat a decision that the daemon already made.

use anyhow::Result;
use crucible_core::proposal::{Proposal, ProposalId};

use super::DaemonClient;

/// Request for `proposal.list`.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProposalListRequest {
    /// Also list the proposals that left the Inbox.
    #[serde(default)]
    pub all: bool,
}

/// Request for `proposal.get`, `proposal.accept` and `proposal.dismiss`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProposalIdRequest {
    pub id: ProposalId,
}

/// Request for `proposal.reject`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProposalRejectRequest {
    pub id: ProposalId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Request for `proposal.resolve`: the text that the user settled for one
/// conflicted file.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProposalResolveRequest {
    pub id: ProposalId,
    /// The path relative to the kiln root, as the proposal names it.
    pub path: String,
    pub text: String,
}

impl DaemonClient {
    /// `proposal.list`: the proposals in the Inbox, oldest first. With
    /// `all`, every stored proposal.
    pub async fn proposal_list(&self, all: bool) -> Result<Vec<Proposal>> {
        self.typed_call_with_retry("proposal.list", ProposalListRequest { all })
            .await
    }

    /// `proposal.get`: one proposal.
    pub async fn proposal_get(&self, id: &ProposalId) -> Result<Proposal> {
        self.typed_call_with_retry("proposal.get", ProposalIdRequest { id: *id })
            .await
    }

    /// `proposal.accept`: write every file of the proposal.
    pub async fn proposal_accept(&self, id: &ProposalId) -> Result<Proposal> {
        self.typed_call("proposal.accept", ProposalIdRequest { id: *id })
            .await
    }

    /// `proposal.reject`: reject the proposal, with an optional reason.
    pub async fn proposal_reject(&self, id: &ProposalId, reason: Option<&str>) -> Result<Proposal> {
        self.typed_call(
            "proposal.reject",
            ProposalRejectRequest {
                id: *id,
                reason: reason.map(str::to_string),
            },
        )
        .await
    }

    /// `proposal.dismiss`: take the proposal out of the Inbox with no
    /// decision.
    pub async fn proposal_dismiss(&self, id: &ProposalId) -> Result<Proposal> {
        self.typed_call("proposal.dismiss", ProposalIdRequest { id: *id })
            .await
    }

    /// `proposal.resolve`: write the settled text of one conflicted file.
    pub async fn proposal_resolve(
        &self,
        id: &ProposalId,
        path: &str,
        text: &str,
    ) -> Result<Proposal> {
        self.typed_call(
            "proposal.resolve",
            ProposalResolveRequest {
                id: *id,
                path: path.to_string(),
                text: text.to_string(),
            },
        )
        .await
    }
}
