//! The comment store: the comments of each diffset, one JSON file for each
//! diffset under `<data_home>/diff-comments/`.
//!
//! A diffset owns its comments, not a session. A branch diff and a proposal
//! have no session, so a store keyed by session cannot hold their comments.
//!
//! Each file is a [`RegistryStore`]: a sidecar lock, a read, a change and an
//! atomic rename. Thus two writers cannot lose a comment, and a reader
//! without the lock sees a complete file.

use crucible_core::protocol::requests::ListedComment;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use crucible_core::diff::{project, DiffsetId, DiffsetSource, Projection};
use crucible_core::proposal::ProposalId;
use crucible_core::session::{Comment, LineRange, SessionId};
use serde::{Deserialize, Serialize};

use crate::registry_store::RegistryStore;

/// The name of the store directory in the daemon data home.
const DIR: &str = "diff-comments";

/// Where a daemon with the data home `data_home` keeps its comments.
pub fn comments_root(data_home: &Path) -> PathBuf {
    data_home.join(DIR)
}

/// The comment store of the daemon whose review snapshots are in
/// `snapshot_root`.
///
/// The daemon passes `<data_home>/review-snapshots` as the snapshot root (see
/// [`crate::review::snapshot_root`]), so the store is
/// `<data_home>/diff-comments`. A snapshot root with no parent keeps the
/// store inside itself; the snapshot store does not read that name.
pub fn root_beside_snapshots(snapshot_root: &Path) -> PathBuf {
    match snapshot_root.parent() {
        Some(data_home) => comments_root(data_home),
        None => snapshot_root.join(DIR),
    }
}

/// One file of the store: the comments of one diffset.
#[derive(Debug, Default, Serialize, Deserialize)]
struct DiffsetComments {
    /// The daemon copied the comments of the session journal into this file.
    /// Only a session record has a journal. See [`CommentStore::migrate`].
    #[serde(default)]
    journal_migrated: bool,
    /// The source of the diffset. A branch id is a hash, so only this field
    /// names the branch of a comment that a client finds by its id alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<DiffsetSource>,
    #[serde(default)]
    comments: Vec<Comment>,
}

/// The source that a diffset id names, for a kind whose id holds its source.
///
/// A session record and a proposal put their id in the diffset id. A branch
/// id is a hash, so it gives `None`.
fn source_of_id(id: &str) -> Option<DiffsetSource> {
    if let Some(session) = id.strip_prefix("session-") {
        return SessionId::parse(session)
            .ok()
            .map(|session| DiffsetSource::SessionRecord { session });
    }
    let proposal = id.strip_prefix("proposal-")?;
    proposal
        .parse::<ProposalId>()
        .ok()
        .map(|id| DiffsetSource::Proposal { id })
}

/// The comments of every diffset.
#[derive(Debug, Clone)]
pub struct CommentStore {
    dir: PathBuf,
}

impl CommentStore {
    /// A store over `dir`. The store creates `dir` at the first write.
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// The comments of `diffset`, oldest first. A diffset with no file has
    /// no comments.
    pub fn list(&self, diffset: &DiffsetId) -> Result<Vec<Comment>> {
        Ok(self.file(diffset)?.read()?.comments)
    }

    /// The comments of `diffset`, oldest first, each projected onto the
    /// current text of its side.
    ///
    /// `side_text` gives the current text of the side of a comment. `None`
    /// means that the file is absent on that side, so the comment is
    /// outdated. The store does not change: a later text can hold the
    /// quoted text again.
    pub fn list_projected(
        &self,
        diffset: &DiffsetId,
        mut side_text: impl FnMut(&Comment) -> Option<String>,
    ) -> Result<Vec<ListedComment>> {
        Ok(self
            .list(diffset)?
            .into_iter()
            .map(|mut comment| {
                let projection = match side_text(&comment) {
                    Some(text) => project(&comment.quoted, comment.line_range, &text),
                    None => Projection::Outdated,
                };
                let outdated = match projection {
                    Projection::Kept => false,
                    Projection::Moved(range) => {
                        comment.line_range = range;
                        false
                    }
                    Projection::Outdated => true,
                };
                ListedComment { comment, outdated }
            })
            .collect())
    }

    /// Store a new comment under its own diffset.
    pub fn add(&self, comment: &Comment) -> Result<()> {
        self.file(&comment.diffset)?.update(|file| {
            if file.comments.iter().any(|c| c.id == comment.id) {
                bail!("comment {} is already stored", comment.id);
            }
            file.comments.push(comment.clone());
            Ok(())
        })
    }

    /// Keep the source of a diffset beside its comments, so that a later
    /// search by comment id can name the diffset.
    pub fn remember_source(&self, source: &DiffsetSource) -> Result<()> {
        self.file(&source.id())?.update(|file| {
            file.source = Some(source.clone());
            Ok(())
        })
    }

    /// Find a comment by its id in every diffset, with the source of its
    /// diffset.
    ///
    /// The source is `None` for a branch comment that the store kept before
    /// it kept sources. A missing store directory holds no comment.
    pub fn find(&self, comment_id: &str) -> Result<Option<(Comment, Option<DiffsetSource>)>> {
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        for entry in entries {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let file = RegistryStore::<DiffsetComments>::new(path.clone()).read()?;
            if let Some(comment) = file.comments.into_iter().find(|c| c.id == comment_id) {
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default();
                let source = file.source.or_else(|| source_of_id(stem));
                return Ok(Some((comment, source)));
            }
        }
        Ok(None)
    }

    /// Mark a comment of `diffset` resolved. The result is `false` when the
    /// diffset has no comment with that id.
    pub fn resolve(&self, diffset: &DiffsetId, comment_id: &str) -> Result<bool> {
        self.file(diffset)?.update(|file| {
            Ok(
                match file.comments.iter_mut().find(|c| c.id == comment_id) {
                    Some(comment) => {
                        comment.resolved = true;
                        true
                    }
                    None => false,
                },
            )
        })
    }

    /// Remove a comment of `diffset` from the store. The result is `false`
    /// when the diffset has no comment with that id.
    ///
    /// Delete is not resolve. Resolve keeps the record of a settled remark;
    /// delete says that the author never wrote the remark, so nothing of it
    /// stays behind.
    pub fn delete(&self, diffset: &DiffsetId, comment_id: &str) -> Result<bool> {
        self.file(diffset)?.update(|file| {
            let before = file.comments.len();
            file.comments.retain(|c| c.id != comment_id);
            Ok(file.comments.len() != before)
        })
    }

    /// Whether the journal comments of `diffset` are already in the store.
    pub fn is_migrated(&self, diffset: &DiffsetId) -> Result<bool> {
        Ok(self.file(diffset)?.read()?.journal_migrated)
    }

    /// Copy the comments of a session journal into the file of `diffset`,
    /// one time only.
    ///
    /// The result is `false` when an earlier call did the copy. Then the
    /// store keeps its own comments: a comment that a user resolved after
    /// the copy does not open again. A comment whose id is already stored is
    /// not copied a second time.
    pub fn migrate(&self, diffset: &DiffsetId, comments: Vec<Comment>) -> Result<bool> {
        self.file(diffset)?.update(|file| {
            if file.journal_migrated {
                return Ok(false);
            }
            for comment in comments {
                if !file.comments.iter().any(|c| c.id == comment.id) {
                    file.comments.push(comment);
                }
            }
            file.journal_migrated = true;
            Ok(true)
        })
    }

    /// The file of one diffset.
    ///
    /// A diffset id arrives from the wire, so the store refuses an id that
    /// could name a file outside its directory.
    fn file(&self, diffset: &DiffsetId) -> Result<RegistryStore<DiffsetComments>> {
        let id = diffset.as_str();
        let safe = !id.is_empty()
            && !id.starts_with('.')
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
        if !safe {
            bail!("diffset id {id:?} is not a valid file name");
        }
        Ok(RegistryStore::new(self.dir.join(format!("{id}.json"))))
    }
}

/// The lines of `text` in `range`, with their line ends.
///
/// `range` is 1-based with an exclusive end. A range past the end of the
/// text gives the lines that exist, or an empty string.
pub fn quoted_lines(text: &str, range: LineRange) -> String {
    text.split_inclusive('\n')
        .skip(range.start.saturating_sub(1) as usize)
        .take(range.len() as usize)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::session::{
        CommentAnchor, CommentAuthor, CommentSide, PhysicalRoot, SessionId, SnapshotId,
    };
    use tempfile::TempDir;

    fn session(id: &str) -> DiffsetId {
        DiffsetId::for_session(&SessionId::parse(id).unwrap())
    }

    fn comment(diffset: &DiffsetId, body: &str) -> Comment {
        Comment::new(
            diffset.clone(),
            CommentAnchor::Snapshot(SnapshotId::git("0".repeat(40))),
            PhysicalRoot::from_top_level("/repo"),
            "src/a.rs",
            CommentSide::Current,
            LineRange::new(1, 2),
            "a\n",
            body,
            CommentAuthor::Human,
        )
    }

    #[test]
    fn a_comment_is_stored_by_diffset() {
        let dir = TempDir::new().unwrap();
        let store = CommentStore::new(dir.path().join(DIR));
        let first = session("s-1");
        let second = session("s-2");
        let kept = comment(&first, "one");
        store.add(&kept).unwrap();
        store.add(&comment(&second, "two")).unwrap();

        assert_eq!(store.list(&first).unwrap(), vec![kept.clone()]);
        assert_eq!(store.list(&second).unwrap().len(), 1);
        assert!(
            dir.path().join(DIR).join("session-s-1.json").is_file(),
            "the file is not named for the diffset"
        );

        // A second store over the same directory reads the same comments.
        let reopened = CommentStore::new(dir.path().join(DIR));
        assert!(reopened.resolve(&first, &kept.id).unwrap());
        assert!(reopened.list(&first).unwrap()[0].resolved);
        assert!(!reopened.resolve(&second, &kept.id).unwrap());
    }

    #[test]
    fn a_deleted_comment_leaves_the_store() {
        let dir = TempDir::new().unwrap();
        let store = CommentStore::new(dir.path().join(DIR));
        let first = session("s-1");
        let second = session("s-2");
        let gone = comment(&first, "gone");
        let kept = comment(&first, "kept");
        store.add(&gone).unwrap();
        store.add(&kept).unwrap();

        // The comment of another diffset stays where it is.
        assert!(!store.delete(&second, &gone.id).unwrap());
        assert_eq!(store.list(&first).unwrap().len(), 2);

        assert!(store.delete(&first, &gone.id).unwrap());
        assert_eq!(store.list(&first).unwrap(), vec![kept]);
        // A second delete of the same id finds nothing.
        assert!(!store.delete(&first, &gone.id).unwrap());
        // A deleted comment is not resolved: no listing holds it again.
        assert!(!store.resolve(&first, &gone.id).unwrap());
    }

    #[test]
    fn a_comment_is_found_by_its_id_with_its_source() {
        let dir = TempDir::new().unwrap();
        let store = CommentStore::new(dir.path().join(DIR));
        assert!(
            store.find("none").unwrap().is_none(),
            "no directory, no comment"
        );

        let record = session("s-1");
        let on_record = comment(&record, "record");
        store.add(&on_record).unwrap();
        let branch = DiffsetSource::Branch {
            root: PhysicalRoot::from_top_level("/repo"),
            base: "main".into(),
            head: None,
        };
        let on_branch = comment(&branch.id(), "branch");
        store.add(&on_branch).unwrap();

        // A session record names its source in its id.
        let (found, source) = store.find(&on_record.id).unwrap().unwrap();
        assert_eq!(found, on_record);
        assert_eq!(
            source,
            Some(DiffsetSource::SessionRecord {
                session: SessionId::parse("s-1").unwrap()
            })
        );
        // A branch id is a hash: the store names the source only after it
        // keeps it.
        assert_eq!(store.find(&on_branch.id).unwrap().unwrap().1, None);
        store.remember_source(&branch).unwrap();
        assert_eq!(store.find(&on_branch.id).unwrap().unwrap().1, Some(branch));
        assert!(store.find("missing").unwrap().is_none());
    }

    #[test]
    fn quoted_lines_take_the_range_with_its_line_ends() {
        let text = "a\nb\nc\nd";
        assert_eq!(quoted_lines(text, LineRange::new(2, 4)), "b\nc\n");
        assert_eq!(quoted_lines(text, LineRange::new(4, 5)), "d");
        assert_eq!(quoted_lines(text, LineRange::new(3, 3)), "");
        assert_eq!(quoted_lines(text, LineRange::new(9, 12)), "");
    }

    #[test]
    fn a_diffset_id_that_leaves_the_store_is_refused() {
        let dir = TempDir::new().unwrap();
        let store = CommentStore::new(dir.path().join(DIR));
        for id in ["../x", "a/b", ".hidden", ""] {
            let diffset: DiffsetId = serde_json::from_value(serde_json::json!(id)).unwrap();
            assert!(store.list(&diffset).is_err(), "{id:?} was accepted");
        }
    }

    #[test]
    fn the_store_migrates_a_diffset_once() {
        let dir = TempDir::new().unwrap();
        let store = CommentStore::new(dir.path().join(DIR));
        let diffset = session("s-1");
        let old = comment(&diffset, "old");

        assert!(!store.is_migrated(&diffset).unwrap());
        assert!(store.migrate(&diffset, vec![old.clone()]).unwrap());
        assert!(store.is_migrated(&diffset).unwrap());
        assert!(!store.migrate(&diffset, vec![old.clone()]).unwrap());
        assert_eq!(store.list(&diffset).unwrap(), vec![old]);
    }

    #[test]
    fn listed_comments_follow_their_text() {
        let dir = TempDir::new().unwrap();
        let store = CommentStore::new(dir.path().join(DIR));
        let diffset = session("s-1");
        let kept = comment(&diffset, "kept");
        let mut moved = comment(&diffset, "moved");
        moved.path = "src/b.rs".into();
        let mut gone = comment(&diffset, "gone");
        gone.path = "src/gone.rs".into();
        for c in [&kept, &moved, &gone] {
            store.add(c).unwrap();
        }

        let listed = store
            .list_projected(&diffset, |c| match c.path.as_str() {
                "src/a.rs" => Some("a\nb\n".into()),
                "src/b.rs" => Some("new\na\n".into()),
                _ => None,
            })
            .unwrap();

        let ranges: Vec<_> = listed
            .iter()
            .map(|l| (l.comment.body.as_str(), l.comment.line_range, l.outdated))
            .collect();
        assert_eq!(
            ranges,
            vec![
                ("kept", LineRange::new(1, 2), false),
                ("moved", LineRange::new(2, 3), false),
                ("gone", LineRange::new(1, 2), true),
            ]
        );
        // The projection does not write the moved range back.
        assert_eq!(
            store.list(&diffset).unwrap()[1].line_range,
            LineRange::new(1, 2)
        );
    }
}
