//! Proposal RPCs, forwarded with the daemon's types.
//!
//! Only the reads may replay. A lost answer to a decision must surface as an
//! unknown outcome, never as a second decision.

use super::daemon::ReconnectingDaemon;
use crucible_core::proposal::{Proposal, ProposalId};

impl ReconnectingDaemon {
    forward_rpc! {
        Safe ProposalList =>
        proposal_list(all: bool)
        -> Vec<Proposal> = proposal_list(all);
    }

    forward_rpc! {
        Safe ProposalGet =>
        proposal_get(id: &ProposalId)
        -> Proposal = proposal_get(&id);
    }

    forward_rpc! {
        Once ProposalAccept =>
        proposal_accept(id: &ProposalId)
        -> Proposal = proposal_accept(&id);
    }

    forward_rpc! {
        Once ProposalReject =>
        proposal_reject(id: &ProposalId, reason: Option<&str> => reason.map(str::to_owned))
        -> Proposal = proposal_reject(&id, reason.as_deref());
    }

    forward_rpc! {
        Once ProposalDismiss =>
        proposal_dismiss(id: &ProposalId)
        -> Proposal = proposal_dismiss(&id);
    }

    forward_rpc! {
        Once ProposalResolve =>
        proposal_resolve(id: &ProposalId, path: &str, text: &str)
        -> Proposal = proposal_resolve(&id, &path, &text);
    }
}
