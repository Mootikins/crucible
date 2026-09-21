//! Commands for the daemon-owned text write path.
use crate::note_edit::AnchoredEdit;
use serde::{Deserialize, Serialize};

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
