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
use crate::session::{Comment, LineRange, PhysicalRoot, SessionId};

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

/// A reference to one stored comment that a client attaches to a chat
/// message.
///
/// The client sends only the reference. The daemon finds the comment in the
/// diffset of `source` and gives the agent the comment as context.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CommentRef {
    /// The id of the comment.
    pub id: String,
    /// The source of the diffset that owns the comment.
    pub source: DiffsetSource,
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
    /// The roots that the diffset leaves out, because the daemon cannot read
    /// them. Only a session record fills it. A branch and a proposal read
    /// their files directly, so their list is empty.
    pub unreadable_roots: Vec<UnreadableRoot>,
}

/// A root that a diffset leaves out, and the reason.
///
/// The session record compares each root with its session base. When the
/// root or its base snapshot is gone, the record cannot list the files of
/// that root. The client shows the root, so that the user does not read the
/// record as complete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UnreadableRoot {
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub root: PhysicalRoot,
    /// Why the daemon cannot read the root, as a sentence for the user.
    pub reason: String,
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

/// Where the text of a comment is in the current text of its side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Projection {
    /// The text is at the stored range.
    Kept,
    /// The text is at this range, not at the stored range.
    Moved(LineRange),
    /// The current text does not contain the text.
    Outdated,
}

/// Find the quoted text of a comment in the current text of its side.
///
/// The lines compare without their line ends. Thus a last line that gets a
/// line end later still matches. When the text occurs more than once, the
/// match nearest to the stored start wins. An empty quote matches at every
/// line start.
pub fn project(quoted: &str, stored: LineRange, current_text: &str) -> Projection {
    let quote: Vec<&str> = quoted.lines().collect();
    let lines: Vec<&str> = current_text.lines().collect();
    let Some(count) = lines.len().checked_sub(quote.len()) else {
        return Projection::Outdated;
    };
    // Line numbers are 1-based, so the match at index `i` starts at `i + 1`.
    let nearest = (0..=count)
        .filter(|&i| lines[i..i + quote.len()] == quote[..])
        .map(|i| i as u32 + 1)
        .min_by_key(|&start| start.abs_diff(stored.start));
    let Some(start) = nearest else {
        return Projection::Outdated;
    };
    let found = LineRange::new(start, start + quote.len() as u32);
    if found == stored {
        Projection::Kept
    } else {
        Projection::Moved(found)
    }
}

/// The first and the last line of a range, both inclusive.
///
/// `LineRange` has an exclusive end. The text forms show an inclusive end,
/// because a reader counts the last line that the comment covers.
fn inclusive_lines(range: LineRange) -> (u32, u32) {
    (range.start, range.end.saturating_sub(1).max(range.start))
}

/// The lines of a range in text: `start` for one line, else `start-end`.
fn line_span(range: LineRange) -> String {
    match inclusive_lines(range) {
        (start, end) if start == end => start.to_string(),
        (start, end) => format!("{start}-{end}"),
    }
}

/// The reference form of a comment: `path:line` for one line, else
/// `path:start-end` with an inclusive end.
///
/// A chat mention `@path:start-end` attaches the same lines.
pub fn reference(comment: &Comment) -> String {
    format!("{}:{}", comment.path, line_span(comment.line_range))
}

/// The quickfix form of a comment: `path:line: text` for one line, else
/// `path:start: [start-end] text`.
///
/// The Vim default `errorformat` `%f:%l:%m` needs a `:` after the line
/// number, so the first line names only the start line. A range goes at
/// the start of the message. Each further line of the text gets an indent of
/// two spaces, so that it does not look like a new entry.
pub fn quickfix_line(comment: &Comment) -> String {
    let (start, end) = inclusive_lines(comment.line_range);
    let mut lines = comment.body.lines();
    let first = lines.next().unwrap_or_default();
    let mut out = if start == end {
        format!("{}:{start}: {first}", comment.path)
    } else {
        format!("{}:{start}: [{start}-{end}] {first}", comment.path)
    };
    for line in lines {
        out.push_str("\n  ");
        out.push_str(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{CommentAnchor, CommentAuthor, CommentSide};
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
                unreadable_roots: vec![UnreadableRoot {
                    root: root(),
                    reason: "tracked root no longer exists".into(),
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

    #[test]
    fn a_range_that_did_not_move_is_kept() {
        let text = "a\nb\nc\nd\n";
        assert_eq!(
            project("b\nc\n", LineRange::new(2, 4), text),
            Projection::Kept
        );
        // The last line matches without its line end.
        assert_eq!(project("d", LineRange::new(4, 5), text), Projection::Kept);
    }

    #[test]
    fn a_range_moves_when_lines_are_added_above() {
        let text = "new 1\nnew 2\na\nb\nc\n";
        assert_eq!(
            project("b\nc\n", LineRange::new(2, 4), text),
            Projection::Moved(LineRange::new(4, 6))
        );
    }

    #[test]
    fn the_nearest_of_two_matches_wins() {
        let text = "x\ny\nz\nz\nz\nx\ny\n";
        // The matches start at lines 1 and 6. The stored start 5 is nearer to 6.
        assert_eq!(
            project("x\ny\n", LineRange::new(5, 7), text),
            Projection::Moved(LineRange::new(6, 8))
        );
        assert_eq!(
            project("x\ny\n", LineRange::new(2, 4), text),
            Projection::Moved(LineRange::new(1, 3))
        );
    }

    #[test]
    fn a_range_whose_text_is_gone_is_outdated() {
        let text = "a\nb changed\nc\n";
        assert_eq!(
            project("b\nc\n", LineRange::new(2, 4), text),
            Projection::Outdated
        );
        // A quote longer than the text is outdated too.
        assert_eq!(
            project("a\nb\nc\nd\n", LineRange::new(1, 5), "a\n"),
            Projection::Outdated
        );
        assert_eq!(
            project("a\n", LineRange::new(1, 2), ""),
            Projection::Outdated
        );
    }

    fn comment(range: LineRange, body: &str) -> Comment {
        Comment::new(
            DiffsetId::for_session(&SessionId::parse("chat-1").unwrap()),
            CommentAnchor::Commit("abc".into()),
            root(),
            "crates/a/src/lib.rs",
            CommentSide::Current,
            range,
            "",
            body,
            CommentAuthor::Human,
        )
    }

    /// The Vim default `errorformat` `%f:%l:%m` needs this shape.
    fn assert_errorformat(line: &str) {
        let pattern = regex::Regex::new(r"^[^:]+:\d+:.*$").unwrap();
        assert!(pattern.is_match(line), "{line:?} does not match %f:%l:%m");
    }

    #[test]
    fn one_line_comment_matches_errorformat() {
        let line = quickfix_line(&comment(LineRange::new(626, 627), "needs a test"));
        assert_eq!(line, "crates/a/src/lib.rs:626: needs a test");
        assert_errorformat(&line);
    }

    #[test]
    fn range_comment_matches_errorformat() {
        let line = quickfix_line(&comment(
            LineRange::new(626, 629),
            "this deny path needs a test",
        ));
        assert_eq!(
            line,
            "crates/a/src/lib.rs:626: [626-628] this deny path needs a test"
        );
        assert_errorformat(&line);
    }

    #[test]
    fn a_second_line_is_indented() {
        let text = quickfix_line(&comment(LineRange::new(3, 5), "first\nsecond\n\nfourth"));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines,
            [
                "crates/a/src/lib.rs:3: [3-4] first",
                "  second",
                "  ",
                "  fourth"
            ]
        );
        assert_errorformat(lines[0]);
        // Only the first line names a file, so Vim makes one entry.
        for line in &lines[1..] {
            assert!(line.starts_with("  "), "{line:?}");
        }
    }

    #[test]
    fn the_reference_form_is_inclusive() {
        assert_eq!(
            reference(&comment(LineRange::new(626, 629), "x")),
            "crates/a/src/lib.rs:626-628"
        );
        assert_eq!(
            reference(&comment(LineRange::new(12, 13), "x")),
            "crates/a/src/lib.rs:12"
        );
    }
}
