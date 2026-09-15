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
