//! Proposal RPCs, forwarded with the daemon's types.
//!
//! Only the reads may replay. A lost answer to a decision must surface as an
//! unknown outcome, never as a second decision.

use super::daemon::ReconnectingDaemon;
use crucible_core::proposal::{Proposal, ProposalFile, ProposalId};

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
        proposal_accept_files(id: &ProposalId, paths: &[String] => paths.to_vec(), files: &[ProposalFile] => files.to_vec())
        -> Proposal = proposal_accept_files(&id, &paths, &files);
    }

    forward_rpc! {
        Once ProposalReject =>
        proposal_reject_files(
            id: &ProposalId,
            paths: &[String] => paths.to_vec(),
            files: &[ProposalFile] => files.to_vec(),
            reason: Option<&str> => reason.map(str::to_owned),
        )
        -> Proposal = proposal_reject_files(&id, &paths, &files, reason.as_deref());
    }

    forward_rpc! {
        Once ProposalDismiss =>
        proposal_dismiss(id: &ProposalId)
        -> Proposal = proposal_dismiss(&id);
    }

    forward_rpc! {
        Once ProposalResolve =>
        proposal_resolve_file(id: &ProposalId, path: &str, root: Option<&crucible_core::session::PhysicalRoot> => root.cloned(), text: &str)
        -> Proposal = proposal_resolve_file(&id, &path, root.as_ref(), &text);
    }
}
