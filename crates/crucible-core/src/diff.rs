//! The diffset: a set of file changes that the daemon computes and a client
//! renders.
//!
//! A diffset has two parts on the wire. `diff.get` sends the [`Diffset`]: the
//! list of files with their line counts and no text. `diff.file` sends the
//! [`DiffFileText`] of one file when the user expands it. Thus a branch with
//! many files does not send all its texts at once.
//!
//! The daemon computes only the counts. The client computes the hunks.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::proposal::ProposalId;
use crate::session::{PhysicalRoot, SessionId};

/// The identity of one diffset.
///
/// The id derives from the source. Two requests for one source thus get one
/// id, and a client can use the id as the key of a tab.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DiffsetId(String);

impl DiffsetId {
    /// The id of the branch diff of `root` from the merge base with `base`
    /// to `head`, or to the working tree when `head` is `None`.
    pub fn for_branch(root: &PhysicalRoot, base: &str, head: Option<&str>) -> Self {
        // A NUL byte separates the fields, because no path and no git ref
        // contains one. The last byte tells an absent head from an empty one.
        let mut hasher = blake3::Hasher::new();
        hasher.update(root.as_os_str().as_encoded_bytes());
        hasher.update(&[0]);
        hasher.update(base.as_bytes());
        hasher.update(&[0]);
        match head {
            Some(head) => {
                hasher.update(head.as_bytes());
                hasher.update(&[1]);
            }
            None => {
                hasher.update(&[0]);
            }
        }
        let hex = hasher.finalize().to_hex();
        Self(format!("branch-{}", &hex[..32]))
    }

    /// The id of the record of one session.
    pub fn for_session(session: &SessionId) -> Self {
        Self(format!("session-{session}"))
    }

    /// The id of one proposal.
    pub fn for_proposal(id: &ProposalId) -> Self {
        Self(format!("proposal-{id}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DiffsetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Where the two sides of a diffset come from.
///
/// This set is closed. The daemon has one exhaustive match on it, and the web
/// client has one.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(test, derive(strum::EnumDiscriminants))]
#[cfg_attr(test, strum_discriminants(derive(strum::EnumIter)))]
pub enum DiffsetSource {
    /// The merge base of `head` with `base`, to the working tree or to `head`.
    Branch {
        #[cfg_attr(feature = "openapi", schema(value_type = String))]
        root: PhysicalRoot,
        base: String,
        head: Option<String>,
    },
    /// The text before the first tool call of a session, to the files on disk.
    SessionRecord {
        #[cfg_attr(feature = "openapi", schema(value_type = String))]
        session: SessionId,
    },
    /// One proposal. Its files are the files that it proposes to write.
    Proposal { id: ProposalId },
}

impl DiffsetSource {
    /// The id of the diffset that this source gives.
    pub fn id(&self) -> DiffsetId {
        match self {
            Self::Branch { root, base, head } => DiffsetId::for_branch(root, base, head.as_deref()),
            Self::SessionRecord { session } => DiffsetId::for_session(session),
            Self::Proposal { id } => DiffsetId::for_proposal(id),
        }
    }
}

/// How a file changed between the two sides.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    /// The file moved. `from` is the old path, relative to the same root.
    Renamed {
        from: String,
    },
}

/// One file of a diffset, with its counts and no text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DiffFileEntry {
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub root: PhysicalRoot,
    /// The path relative to `root`, as a `Comment` names it.
    pub path: String,
    pub status: FileStatus,
    pub added: u32,
    pub removed: u32,
    /// The file is binary. It has no text.
    pub binary: bool,
    /// One side is larger than [`crate::types::acp::MAX_DIFF_BYTES`]. The
    /// file has no text.
    pub too_large: bool,
}

/// A set of file changes, without the text of the files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Diffset {
    pub id: DiffsetId,
    pub source: DiffsetSource,
    pub files: Vec<DiffFileEntry>,
}

/// The two texts of one file of a diffset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DiffFileText {
    /// `None` when the file is added.
    pub base_text: Option<String>,
    /// `None` when the file is deleted.
    pub current_text: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use strum::IntoEnumIterator;

    fn root() -> PhysicalRoot {
        PhysicalRoot::from_top_level("/repo")
    }

    fn proposal_id() -> ProposalId {
        "6f1c1d2e-3b4a-4c5d-8e9f-0a1b2c3d4e5f".parse().unwrap()
    }

    /// One source of each kind. The match makes a new kind fail to compile
    /// until it has a sample here.
    fn sample_sources() -> Vec<DiffsetSource> {
        DiffsetSourceDiscriminants::iter()
            .map(|kind| match kind {
                DiffsetSourceDiscriminants::Branch => DiffsetSource::Branch {
                    root: root(),
                    base: "main".into(),
                    head: None,
                },
                DiffsetSourceDiscriminants::SessionRecord => DiffsetSource::SessionRecord {
                    session: SessionId::parse("chat-1").unwrap(),
                },
                DiffsetSourceDiscriminants::Proposal => {
                    DiffsetSource::Proposal { id: proposal_id() }
                }
            })
            .collect()
    }

    #[test]
    fn a_diffset_id_is_stable_for_one_source() {
        for source in sample_sources() {
            assert_eq!(source.id(), source.clone().id(), "{source:?}");
        }

        let branch = |base: &str, head: Option<&str>| DiffsetId::for_branch(&root(), base, head);
        assert_eq!(branch("main", None), branch("main", None));
        assert_eq!(branch("main", Some("topic")), branch("main", Some("topic")));
        assert_ne!(branch("main", None), branch("main", Some("")));
        assert_ne!(branch("main", None), branch("master", None));
        assert_ne!(
            branch("main", None),
            DiffsetId::for_branch(&PhysicalRoot::from_top_level("/other"), "main", None)
        );
        // The separator keeps a shifted boundary from giving one id.
        assert_ne!(branch("ab", Some("c")), branch("a", Some("bc")));

        let ids: std::collections::HashSet<_> = sample_sources().iter().map(|s| s.id()).collect();
        assert_eq!(
            ids.len(),
            sample_sources().len(),
            "each kind has its own id"
        );
    }

    #[test]
    fn file_status_renamed_carries_the_old_path() {
        let status = FileStatus::Renamed {
            from: "notes/old.md".into(),
        };
        let value = serde_json::to_value(&status).unwrap();
        assert_eq!(value, json!({ "kind": "renamed", "from": "notes/old.md" }));
        let restored: FileStatus = serde_json::from_value(value).unwrap();
        assert_eq!(restored, status);

        assert_eq!(
            serde_json::to_value(FileStatus::Added).unwrap(),
            json!({ "kind": "added" })
        );
    }

    #[test]
    fn diff_types_round_trip_through_json() {
        for source in sample_sources() {
            let diffset = Diffset {
                id: source.id(),
                source,
                files: vec![DiffFileEntry {
                    root: root(),
                    path: "src/new.rs".into(),
                    status: FileStatus::Renamed {
                        from: "src/old.rs".into(),
                    },
                    added: 3,
                    removed: 1,
                    binary: false,
                    too_large: false,
                }],
            };
            let text = serde_json::to_string(&diffset).unwrap();
            let restored: Diffset = serde_json::from_str(&text).unwrap();
            assert_eq!(restored, diffset);
        }

        assert_eq!(
            serde_json::to_value(DiffsetSource::Branch {
                root: root(),
                base: "main".into(),
                head: Some("topic".into()),
            })
            .unwrap(),
            json!({ "kind": "branch", "root": "/repo", "base": "main", "head": "topic" })
        );
        assert_eq!(
            serde_json::to_value(DiffsetSource::Proposal { id: proposal_id() }).unwrap(),
            json!({ "kind": "proposal", "id": "6f1c1d2e-3b4a-4c5d-8e9f-0a1b2c3d4e5f" })
        );

        let added = DiffFileText {
            base_text: None,
            current_text: Some("new\n".into()),
        };
        let value = serde_json::to_value(&added).unwrap();
        assert_eq!(value, json!({ "base_text": null, "current_text": "new\n" }));
        assert_eq!(
            serde_json::from_value::<DiffFileText>(value).unwrap(),
            added
        );
    }
}
