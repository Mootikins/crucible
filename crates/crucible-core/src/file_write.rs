//! Commands for the daemon-owned file read and text write paths.
use crate::config::ProjectFileAccess;
use crate::note_edit::AnchoredEdit;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// An absolute file path and the change to apply under its write lock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileWriteRequest {
    pub path: String,
    #[serde(flatten)]
    pub change: FileChange,
}

/// A whole text or an anchored batch. Missing bases retain legacy replacement semantics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum FileChange {
    Put {
        content: String,
        #[serde(default)]
        base_hash: Option<String>,
        #[serde(default)]
        base_text: Option<String>,
    },
    Patch {
        edits: Vec<AnchoredEdit>,
        #[serde(default)]
        base_hash: Option<String>,
    },
}

/// An absolute file path for `fs.read`, and the form of the answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileReadRequest {
    pub path: String,
    #[serde(default)]
    pub encoding: FileEncoding,
}

/// How `fs.read` carries the bytes of a file.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileEncoding {
    /// UTF-8 text. The daemon refuses a file that is not UTF-8.
    #[default]
    Text,
    /// Any bytes, in standard base64.
    Base64,
}

/// What `fs.read` answers when the daemon admits the path.
///
/// The daemon owns the rule that selects `root`: the innermost kiln that holds
/// the path, else the innermost registered project or session folder. A
/// client uses `root` as it is and does not decide containment again.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileReadReply {
    /// The canonical root that holds the path.
    pub root: PathBuf,
    /// What the root lets a client do. A kiln is always read-write.
    pub access: ProjectFileAccess,
    /// The path, with its symlinks resolved inside `root`.
    pub path: PathBuf,
    /// The file. `None` when no regular file is at `path`: the path does not
    /// exist, or it is a directory.
    pub content: Option<FileContent>,
}

/// The bytes of a file, in the encoding that the request named.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "encoding", rename_all = "snake_case")]
pub enum FileContent {
    /// UTF-8 text, and the hash that a later `fs.write` names as its base.
    Text { text: String, content_hash: String },
    /// Any bytes, in standard base64.
    Base64 { data: String },
}

/// The disk state that a write expects to find before it writes.
///
/// The wire fields `base_hash` and `base_text` map to this type. The type
/// keeps an absent file and an empty file different: `Absent` expects no
/// file, and `Text` with an empty text expects an empty file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ExpectedBase {
    /// The write replaces the disk text with no check.
    Unchecked,
    /// The file must not exist. A file on disk is a conflict with an empty base.
    Absent,
    /// The disk text must hash to `hash`. A stale base answers `current_hash`,
    /// because the writer has no text to merge from.
    Hash { hash: String },
    /// The disk text must be `text`. A stale base merges `text`, the new
    /// text and the disk text.
    Text { text: String, hash: String },
}

/// Map the wire fields `(base_hash, base_text)` to an expected base.
///
/// An absent file hashes to `""`, so a `""` hash with no text expects no file.
impl From<(Option<String>, Option<String>)> for ExpectedBase {
    fn from((hash, text): (Option<String>, Option<String>)) -> Self {
        match (hash, text) {
            (None, _) => Self::Unchecked,
            (Some(hash), None) if hash.is_empty() => Self::Absent,
            (Some(hash), None) => Self::Hash { hash },
            (Some(hash), Some(text)) => Self::Text { text, hash },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ExpectedBase;

    #[test]
    fn the_wire_fields_map_to_each_expected_base() {
        let s = |v: &str| Some(v.to_string());
        assert_eq!(ExpectedBase::from((None, None)), ExpectedBase::Unchecked);
        assert_eq!(ExpectedBase::from((None, s("t"))), ExpectedBase::Unchecked);
        assert_eq!(ExpectedBase::from((s(""), None)), ExpectedBase::Absent);
        assert_eq!(
            ExpectedBase::from((s("h"), None)),
            ExpectedBase::Hash { hash: "h".into() }
        );
        assert_eq!(
            ExpectedBase::from((s("h"), s(""))),
            ExpectedBase::Text {
                text: String::new(),
                hash: "h".into()
            }
        );
    }
}
