//! Wire types of the `proposals` RPC methods. The client serializes each type,
//! and the daemon handler deserializes the same type.

use crate::proposal::ProposalFile;
use crate::proposal::ProposalId;
/// Request for `proposal.list`.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
#[cfg_attr(feature = "openapi", into_params(parameter_in = Query))]
pub struct ProposalListRequest {
    /// Also list the proposals that left the Inbox: accepted, rejected and
    /// dismissed.
    #[serde(default)]
    pub all: bool,
}

/// Request for `proposal.get` and `proposal.dismiss`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProposalIdRequest {
    pub id: ProposalId,
}

/// Request for `proposal.accept`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProposalAcceptRequest {
    pub id: ProposalId,
    /// The files to write, as the proposal names them. The daemon moves them
    /// into a new proposal and accepts that one. Empty means every file.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
    /// Root-qualified file identities. Cannot be combined with paths.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<ProposalFile>,
}

/// Request for `proposal.reject`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProposalRejectRequest {
    pub id: ProposalId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The files to reject, as the proposal names them. The daemon moves
    /// them into a new proposal and rejects that one. Empty means every file.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
    /// Root-qualified file identities. Cannot be combined with paths.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<ProposalFile>,
}

/// Request for `proposal.resolve`: the text that the user settled for one
/// conflicted file.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProposalResolveRequest {
    pub id: ProposalId,
    /// The path relative to the kiln root, as the proposal names it.
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<crate::session::PhysicalRoot>,
    pub text: String,
}
