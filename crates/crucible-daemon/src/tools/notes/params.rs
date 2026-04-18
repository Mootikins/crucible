//! Parameter types for note CRUD tools.

use crucible_core::serde_helpers::default_true;
use schemars::JsonSchema;
use serde::Deserialize;

/// Custom schema for optional JSON object (used for frontmatter fields).
/// `serde_json::Value` produces an empty schema that llama.cpp can't handle.
/// Returns schema for "object or null" to preserve Option<T> semantics.
pub(super) fn optional_json_object_schema(
    _gen: &mut schemars::SchemaGenerator,
) -> schemars::Schema {
    // Create a schema that represents "any JSON object or null"
    let mut map = serde_json::Map::new();
    map.insert("type".to_owned(), serde_json::json!(["object", "null"]));
    map.into()
}

/// Parameters for creating a note
#[derive(Deserialize, JsonSchema)]
pub struct CreateNoteParams {
    pub(super) path: String,
    pub(super) content: String,
    /// Optional YAML frontmatter to include at the beginning of the note
    #[schemars(schema_with = "optional_json_object_schema")]
    pub(super) frontmatter: Option<serde_json::Value>,
}

/// Parameters for reading a note
#[derive(Deserialize, JsonSchema)]
pub struct ReadNoteParams {
    pub(super) path: String,
    /// Optional 1-indexed line number to start reading from
    pub(super) start_line: Option<usize>,
    /// Optional 1-indexed line number to stop reading at (inclusive)
    pub(super) end_line: Option<usize>,
}

/// Parameters for reading metadata
#[derive(Deserialize, JsonSchema)]
pub struct ReadMetadataParams {
    pub(super) path: String,
}

/// Parameters for updating a note
#[derive(Deserialize, JsonSchema)]
pub struct UpdateNoteParams {
    pub(super) path: String,
    /// New content for the note (if None, content is preserved)
    pub(super) content: Option<String>,
    /// New frontmatter for the note (if None, frontmatter is preserved)
    #[schemars(schema_with = "optional_json_object_schema")]
    pub(super) frontmatter: Option<serde_json::Value>,
}

/// Parameters for deleting a note
#[derive(Deserialize, JsonSchema)]
pub struct DeleteNoteParams {
    pub(super) path: String,
}

/// Parameters for listing notes
#[derive(Deserialize, JsonSchema)]
pub struct ListNotesParams {
    /// Optional folder to search within (relative to kiln root)
    pub(super) folder: Option<String>,
    #[serde(default)]
    pub(super) include_frontmatter: bool,
    #[serde(default = "default_true")]
    pub(super) recursive: bool,
}
