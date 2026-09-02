//! Database domain types consumed across the Crucible workspace.
//!
//! These types originated in the `database` module and are consumed by multiple
//! crates (crucible-cli, crucible-daemon).
//! They live here as the canonical definitions.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Document identifier
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct DocumentId(pub String);

impl std::fmt::Display for DocumentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Database record (row)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    /// Optional record identifier
    pub id: Option<RecordId>,
    /// Record field data (column values or note fields)
    pub data: HashMap<String, serde_json::Value>,
}

impl Record {
    /// Create a new record without an ID
    pub fn new(data: HashMap<String, serde_json::Value>) -> Self {
        Self { id: None, data }
    }
}

/// Record identifier
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct RecordId(pub String);

impl std::fmt::Display for RecordId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for RecordId {
    fn from(s: String) -> Self {
        RecordId(s)
    }
}

impl From<&str> for RecordId {
    fn from(s: &str) -> Self {
        RecordId(s.to_string())
    }
}

/// Query result containing records and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub records: Vec<Record>,
    pub total_count: Option<u64>,
    pub execution_time_ms: Option<u64>,
    pub has_more: bool,
}

/// Search result
///
/// # Why the kiln is a name and not a path
///
/// This struct is the carrier for every retrieval hit that reaches a model
/// prompt, an MCP tool result, a Lua handler, a persisted transcript and the
/// two UIs. It used to hold `kiln_path: Option<PathBuf>`, and every one of
/// those sinks grew its own spelling of the same disclosure — some printed the
/// whole directory, some printed `path.file_name()` and called it a name.
/// A basename is not a kiln name: a kiln registered `notes` at
/// `/home/u/Private Vault` rendered as `Private Vault`.
///
/// Holding a [`KilnName`] instead makes those sinks *unable* to disclose a
/// directory: the resolution happens once, where the search source is built and
/// the registry name is already in hand, and nothing downstream has a path to
/// reach for. `None` means "no name for this kiln" and every renderer must omit
/// the field rather than substitute a placeholder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub document_id: DocumentId,
    pub score: f64,
    pub highlights: Option<Vec<String>>,
    pub snippet: Option<String>,
    /// The registry name of the kiln this hit came from, when the search source
    /// was built from a registered kiln. Never a path, never a basename.
    #[serde(default)]
    pub kiln: Option<crate::config::KilnName>,

    /// The block this hit names, when retrieval reached block granularity.
    ///
    /// `None` means the hit names a whole note — either the kiln predates the
    /// block store and has not been re-indexed, or the caller asked for note
    /// search. A reader can tell the two apart, which matters: a block
    /// reference points somewhere, a note reference points at a file.
    #[serde(default)]
    pub block: Option<BlockRef>,
}

/// Where in a note a hit sits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockRef {
    /// Byte offset where the block starts, relative to the note body.
    pub span_start: usize,
    /// Byte offset one past the block's last byte.
    pub span_end: usize,
    /// The block's kind: `heading`, `paragraph`, `code`, `callout`, …
    pub kind: String,
}
