//! Proposals: writes that wait for the user to accept them.
//!
//! This module holds only the id type now. `DiffsetSource::Proposal` names a
//! proposal, so the id comes before the rest of the proposal value.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
