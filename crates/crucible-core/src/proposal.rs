//! Proposals: writes that wait for the user to accept them.
//!
//! A note write in `propose` mode makes a proposal. The file on disk does not
//! change until a person accepts the proposal. The daemon owns each proposal
//! and keeps its file after it leaves the Inbox, so the rejection history
//! survives.

use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::file_write::ExpectedBase;
use crate::note_merge::Region;
use crate::session::{PhysicalRoot, SessionId};

/// The identity of one proposal.
///
/// The daemon stores each proposal as a file under `<data_root>/proposals/`
/// and uses the id as the file name. A UUID has no path separator and no
/// leading dot, so an id can never name a path outside that directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(value_type = String))]
pub struct ProposalId(Uuid);

impl ProposalId {
    /// Make a new, random id.
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl From<Uuid> for ProposalId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

impl fmt::Display for ProposalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for ProposalId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s).map(Self)
    }
}

/// The writer of a proposal.
///
/// A plugin pass runs a new auxiliary session each time. Thus the daemon names
/// the plugin, not the session, so that a later pass supersedes the proposal
/// of an earlier pass.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ProposalAuthor {
    /// A plugin pass, by the plugin name.
    Plugin { name: String },
    /// A user session or an agent session.
    Session {
        #[cfg_attr(feature = "openapi", schema(value_type = String))]
        id: SessionId,
    },
}

/// The identity of a file within a proposal. A selector grants no write authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ProposalFile {
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub root: PhysicalRoot,
    pub path: String,
}

/// One file that a proposal creates, replaces or deletes.
///
/// A proposal has no rename entry. A move is a deletion of the old path and
/// a creation of the new path in one proposal, which accept writes as one set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ProposedWrite {
    /// The kiln root.
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub root: PhysicalRoot,
    /// The path relative to `root`.
    pub path: String,
    /// The disk state that the writer read before it proposed the write.
    pub base: ExpectedBase,
    /// The whole text that the write puts on disk. Empty for a deletion.
    pub new_text: String,
    /// The write deletes the file.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub remove: bool,
    /// The path, in the same root, of a file that this proposal deletes and
    /// that this file replaces: the two writes are one move. Accept and
    /// reject keep the pair together.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moved_from: Option<String>,
}

/// A merge conflict in one file of a proposal.
///
/// The regions point into `merged_text`, so a client shows them with no
/// second merge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct FileConflict {
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub root: PhysicalRoot,
    pub path: String,
    /// The disk text at the time of the merge.
    pub disk_text: String,
    /// The text of the merge, with the proposed side in each region.
    pub merged_text: String,
    /// Each cluster that the two sides changed differently.
    pub regions: Vec<Region>,
}

/// Where a proposal is in its life.
///
/// `Open`, `Stale`, `Conflicted` and `Superseded` keep the proposal in the
/// Inbox. `Accepted`, `Rejected` and `Dismissed` take it out. No state change
/// removes a proposal file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ProposalState {
    Open,
    /// The disk no longer matches a base. Accept merges.
    Stale,
    /// An accept found a merge conflict and wrote nothing.
    Conflicted {
        files: Vec<FileConflict>,
    },
    Accepted,
    /// The user rejected the proposal. The reason stays with it.
    Rejected {
        reason: Option<String>,
    },
    /// A newer proposal of the same author writes the same path.
    Superseded {
        by: ProposalId,
    },
    /// The user took the proposal out of the Inbox with no decision.
    Dismissed,
}

impl ProposalState {
    /// Whether the proposal waits for the user, and so stays in the Inbox.
    pub fn is_listed(&self) -> bool {
        match self {
            Self::Open | Self::Stale | Self::Conflicted { .. } | Self::Superseded { .. } => true,
            Self::Accepted | Self::Rejected { .. } | Self::Dismissed => false,
        }
    }

    /// Whether a newer write can still replace the proposal: it is not
    /// decided and no newer proposal replaces it.
    pub fn is_pending(&self) -> bool {
        match self {
            Self::Open | Self::Stale | Self::Conflicted { .. } => true,
            Self::Superseded { .. } | Self::Accepted | Self::Rejected { .. } | Self::Dismissed => {
                false
            }
        }
    }
}

/// A set of note writes that waits for the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Proposal {
    pub id: ProposalId,
    pub author: ProposalAuthor,
    /// The session that made the proposal, for the transcript.
    #[cfg_attr(feature = "openapi", schema(value_type = Option<String>))]
    pub session: Option<SessionId>,
    pub title: String,
    pub rationale: Option<String>,
    pub created_at: DateTime<Utc>,
    pub state: ProposalState,
    pub writes: Vec<ProposedWrite>,
}

impl Proposal {
    /// Whether the proposal writes `path` under `root`.
    pub fn writes_path(&self, root: &PhysicalRoot, path: &str) -> bool {
        self.writes
            .iter()
            .any(|w| w.root == *root && w.path == path)
    }

    /// The write that moves `write` elsewhere, when `write` is the deletion
    /// half of a move.
    pub fn moved_to(&self, write: &ProposedWrite) -> Option<&ProposedWrite> {
        write.remove.then_some(())?;
        self.writes
            .iter()
            .find(|w| w.root == write.root && w.moved_from.as_deref() == Some(&write.path))
    }

    /// The deletion that `write` completes, when `write` is the new half of
    /// a move.
    pub fn moved_from(&self, write: &ProposedWrite) -> Option<&ProposedWrite> {
        let from = write.moved_from.as_deref()?;
        self.writes
            .iter()
            .find(|w| w.root == write.root && w.remove && w.path == from)
    }

    /// The writes a person reviews: a move is one change, its new half.
    pub fn changes(&self) -> impl Iterator<Item = &ProposedWrite> {
        self.writes.iter().filter(|w| self.moved_to(w).is_none())
    }

    /// The title that names what the writes do.
    pub fn describe(&self) -> String {
        let changes: Vec<_> = self.changes().collect();
        match changes.as_slice() {
            [only] => match &only.moved_from {
                Some(from) => format!("Move {from} to {}", only.path),
                None if only.remove => format!("Delete {}", only.path),
                None => format!("Change {}", only.path),
            },
            changes => format!("Change {} notes", changes.len()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_proposal_state_carries_its_kind_on_the_wire() {
        let by = ProposalId::generate();
        let value = serde_json::to_value(ProposalState::Superseded { by }).unwrap();
        assert_eq!(value["kind"], "superseded");
        assert_eq!(value["by"], by.to_string());
        let value = serde_json::to_value(ProposalState::Rejected { reason: None }).unwrap();
        assert_eq!(value["kind"], "rejected");
    }

    #[test]
    fn only_a_decided_or_dismissed_proposal_leaves_the_inbox() {
        let by = ProposalId::generate();
        let listed = [
            ProposalState::Open,
            ProposalState::Stale,
            ProposalState::Conflicted { files: vec![] },
            ProposalState::Superseded { by },
        ];
        let gone = [
            ProposalState::Accepted,
            ProposalState::Rejected { reason: None },
            ProposalState::Dismissed,
        ];
        assert!(listed.iter().all(ProposalState::is_listed));
        assert!(!gone.iter().any(ProposalState::is_listed));
    }
}
