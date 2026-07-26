//! Event payload types
//!
//! Structured payloads for various session events.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::path::PathBuf;

/// Payload for note events containing parsed note data.
///
/// This is a simplified payload for event transmission. It captures the essential
/// information extracted from a parsed note without the full AST representation.
///
/// # Example
///
/// ```ignore
/// use crucible_core::events::{SessionEvent, NotePayload};
/// use std::path::PathBuf;
///
/// let payload = NotePayload::new("notes/test.md", "Test Note")
///     .with_tags(vec!["rust".into(), "test".into()])
///     .with_wikilinks(vec!["other-note".into()]);
///
/// let event = SessionEvent::NoteParsed {
///     path: PathBuf::from("notes/test.md"),
///     block_count: 5,
///     payload: Some(payload),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NotePayload {
    /// Note path (relative to kiln root).
    pub path: String,

    /// Title (from frontmatter or filename).
    pub title: String,

    /// Frontmatter as JSON value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frontmatter: Option<JsonValue>,

    /// Tags extracted from content and frontmatter.
    #[serde(default)]
    pub tags: Vec<String>,

    /// Wikilink targets found in the note.
    #[serde(default)]
    pub wikilinks: Vec<String>,

    /// Content hash for change detection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,

    /// File size in bytes.
    #[serde(default)]
    pub file_size: u64,

    /// Word count of the content.
    #[serde(default)]
    pub word_count: usize,
}

impl NotePayload {
    /// Create a new payload with required fields.
    pub fn new(path: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            title: title.into(),
            frontmatter: None,
            tags: Vec::new(),
            wikilinks: Vec::new(),
            content_hash: None,
            file_size: 0,
            word_count: 0,
        }
    }

    /// Set frontmatter JSON value.
    pub fn with_frontmatter(mut self, frontmatter: JsonValue) -> Self {
        self.frontmatter = Some(frontmatter);
        self
    }

    /// Set tags.
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Set wikilink targets.
    pub fn with_wikilinks(mut self, wikilinks: Vec<String>) -> Self {
        self.wikilinks = wikilinks;
        self
    }

    /// Set content hash.
    pub fn with_content_hash(mut self, hash: impl Into<String>) -> Self {
        self.content_hash = Some(hash.into());
        self
    }

    /// Set file size.
    pub fn with_file_size(mut self, size: u64) -> Self {
        self.file_size = size;
        self
    }

    /// Set word count.
    pub fn with_word_count(mut self, count: usize) -> Self {
        self.word_count = count;
        self
    }
}

impl Default for NotePayload {
    fn default() -> Self {
        Self::new("", "")
    }
}

/// Session configuration for SessionStarted events.
///
/// This is a simplified version of session config for event serialization.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SessionEventConfig {
    /// Unique session identifier.
    pub session_id: String,
    /// Session folder path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<PathBuf>,
    /// Maximum context tokens before compaction.
    #[serde(default)]
    pub max_context_tokens: usize,
    /// Optional system prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
}

impl SessionEventConfig {
    /// Create a new session config with the given ID.
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            folder: None,
            max_context_tokens: 100_000,
            system_prompt: None,
        }
    }

    /// Set the folder path.
    pub fn with_folder(mut self, folder: impl Into<PathBuf>) -> Self {
        self.folder = Some(folder.into());
        self
    }

    /// Set the maximum context tokens.
    pub fn with_max_context_tokens(mut self, tokens: usize) -> Self {
        self.max_context_tokens = tokens;
        self
    }

    /// Set the system prompt.
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }
}
