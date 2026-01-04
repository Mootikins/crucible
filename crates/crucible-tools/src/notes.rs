//! Note CRUD operations tools
//!
//! This module provides simple filesystem-based note CRUD tools.
//!
//! # `NoteStore` Integration
//!
//! `NoteTools` can optionally use a `NoteStore` for faster metadata reads. When a
//! `NoteStore` is provided:
//!
//! - `read_metadata` uses the indexed metadata instead of parsing from filesystem
//! - `list_notes` uses the indexed note list for faster directory listing
//!
//! CRUD operations (create, read, update, delete) always use the filesystem directly
//! since the filesystem is the source of truth.

#![allow(missing_docs)]

use crucible_core::storage::NoteStore;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{model::CallToolResult, tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::Path;
use std::sync::Arc;
use walkdir::WalkDir;

/// Custom schema for optional JSON object (used for frontmatter fields).
/// `serde_json::Value` produces an empty schema that llama.cpp can't handle.
/// Returns schema for "object or null" to preserve Option<T> semantics.
fn optional_json_object_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    // Create a schema that represents "any JSON object or null"
    let mut map = serde_json::Map::new();
    map.insert("type".to_owned(), serde_json::json!(["object", "null"]));
    map.into()
}

#[derive(Clone)]
#[allow(missing_docs)]
pub struct NoteTools {
    kiln_path: String,
    /// Optional `NoteStore` for faster metadata reads
    note_store: Option<Arc<dyn NoteStore>>,
}

fn ensure_md_suffix(path: String) -> String {
    let pb = Path::new(&path);
    if pb.extension().is_some() {
        path
    } else {
        format!("{path}.md")
    }
}

/// Parameters for creating a note
#[derive(Deserialize, JsonSchema)]
pub struct CreateNoteParams {
    path: String,
    content: String,
    /// Optional YAML frontmatter to include at the beginning of the note
    #[schemars(schema_with = "optional_json_object_schema")]
    frontmatter: Option<serde_json::Value>,
}

/// Parameters for reading a note
#[derive(Deserialize, JsonSchema)]
pub struct ReadNoteParams {
    path: String,
    /// Optional 1-indexed line number to start reading from
    start_line: Option<usize>,
    /// Optional 1-indexed line number to stop reading at (inclusive)
    end_line: Option<usize>,
}

/// Parameters for reading metadata
#[derive(Deserialize, JsonSchema)]
pub struct ReadMetadataParams {
    path: String,
}

/// Parameters for updating a note
#[derive(Deserialize, JsonSchema)]
pub struct UpdateNoteParams {
    path: String,
    /// New content for the note (if None, content is preserved)
    content: Option<String>,
    /// New frontmatter for the note (if None, frontmatter is preserved)
    #[schemars(schema_with = "optional_json_object_schema")]
    frontmatter: Option<serde_json::Value>,
}

/// Parameters for deleting a note
#[derive(Deserialize, JsonSchema)]
pub struct DeleteNoteParams {
    path: String,
}

/// Parameters for listing notes
#[derive(Deserialize, JsonSchema)]
pub struct ListNotesParams {
    /// Optional folder to search within (relative to kiln root)
    folder: Option<String>,
    #[serde(default)]
    include_frontmatter: bool,
    #[serde(default = "default_true")]
    recursive: bool,
}

fn default_true() -> bool {
    true
}

impl NoteTools {
    #[allow(missing_docs)]
    #[must_use]
    pub fn new(kiln_path: String) -> Self {
        Self {
            kiln_path,
            note_store: None,
        }
    }

    /// Create `NoteTools` with a `NoteStore` for faster metadata operations
    ///
    /// When a `NoteStore` is provided, `read_metadata` and `list_notes` use the
    /// indexed metadata instead of parsing from the filesystem.
    #[must_use]
    pub fn with_note_store(kiln_path: String, note_store: Arc<dyn NoteStore>) -> Self {
        Self {
            kiln_path,
            note_store: Some(note_store),
        }
    }
}

#[tool_router]
impl NoteTools {
    #[tool(description = "Create a new note in the kiln")]
    pub async fn create_note(
        &self,
        params: Parameters<CreateNoteParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let params = params.0;
        let path = ensure_md_suffix(params.path);
        let content = params.content;
        let frontmatter = params.frontmatter;

        // Security: Validate path to prevent traversal attacks
        let full_path = validate_path_within_kiln(&self.kiln_path, &path)?;

        // Build final content with optional frontmatter
        let final_content = if let Some(fm) = frontmatter {
            let fm_str = serialize_frontmatter_to_yaml(&fm)
                .map_err(|e| rmcp::ErrorData::internal_error(e, None))?;
            format!("{fm_str}{content}")
        } else {
            content
        };

        std::fs::write(&full_path, &final_content).map_err(|e| {
            rmcp::ErrorData::internal_error(format!("Failed to write file: {e}"), None)
        })?;

        // TODO: Notify parser for reprocessing

        Ok(CallToolResult::success(vec![rmcp::model::Content::json(
            serde_json::json!({
                "path": path,
                "full_path": full_path.to_string_lossy(),
                "status": "created"
            }),
        )?]))
    }

    #[tool(description = "Read note content with optional line range")]
    pub async fn read_note(
        &self,
        params: Parameters<ReadNoteParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let params = params.0;
        let path = ensure_md_suffix(params.path);

        // Security: Validate path to prevent traversal attacks
        let full_path = validate_path_within_kiln(&self.kiln_path, &path)?;

        if !full_path.exists() {
            return Err(rmcp::ErrorData::invalid_params(
                format!("File not found: {path}"),
                None,
            ));
        }

        let content = std::fs::read_to_string(&full_path).map_err(|e| {
            rmcp::ErrorData::internal_error(format!("Failed to read file: {e}"), None)
        })?;

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        // Apply line range if specified
        let (content_slice, lines_returned) = match (params.start_line, params.end_line) {
            (Some(start), Some(end)) => {
                let start_idx = (start.saturating_sub(1)).min(total_lines);
                let end_idx = end.min(total_lines);
                let slice = lines[start_idx..end_idx].join("\n");
                (slice, end_idx - start_idx)
            }
            (None, Some(end)) => {
                let end_idx = end.min(total_lines);
                let slice = lines[..end_idx].join("\n");
                (slice, end_idx)
            }
            (Some(start), None) => {
                let start_idx = (start.saturating_sub(1)).min(total_lines);
                let slice = lines[start_idx..].join("\n");
                (slice, total_lines - start_idx)
            }
            (None, None) => (content, total_lines),
        };

        Ok(CallToolResult::success(vec![rmcp::model::Content::json(
            serde_json::json!({
                "path": path,
                "content": content_slice,
                "total_lines": total_lines,
                "lines_returned": lines_returned
            }),
        )?]))
    }

    #[tool(description = "Read note metadata without loading full content")]
    pub async fn read_metadata(
        &self,
        params: Parameters<ReadMetadataParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let params = params.0;
        let path = ensure_md_suffix(params.path);

        // Security: Validate path to prevent traversal attacks
        let full_path = validate_path_within_kiln(&self.kiln_path, &path)?;

        // Try NoteStore first for faster indexed access
        if let Some(ref note_store) = self.note_store {
            if let Ok(Some(note_record)) = note_store.get(&path).await {
                // Build frontmatter from NoteRecord
                let mut frontmatter = serde_json::json!({
                    "title": note_record.title,
                    "tags": note_record.tags,
                });

                // Merge additional properties
                if let Some(obj) = frontmatter.as_object_mut() {
                    for (k, v) in &note_record.properties {
                        obj.insert(k.clone(), v.clone());
                    }
                }

                let modified = note_record
                    .updated_at
                    .timestamp()
                    .try_into()
                    .ok()
                    .map(|ts: u64| ts);

                return Ok(CallToolResult::success(vec![rmcp::model::Content::json(
                    serde_json::json!({
                        "path": path,
                        "frontmatter": frontmatter,
                        "stats": {
                            "links_count": note_record.links_to.len(),
                            "tags_count": note_record.tags.len(),
                            "has_embedding": note_record.has_embedding(),
                        },
                        "modified": modified,
                        "source": "index"
                    }),
                )?]));
            }
            // Note not found in store, fall through to filesystem
        }

        // Fallback: read from filesystem
        if !full_path.exists() {
            return Err(rmcp::ErrorData::invalid_params(
                format!("File not found: {path}"),
                None,
            ));
        }

        let content = std::fs::read_to_string(&full_path).map_err(|e| {
            rmcp::ErrorData::internal_error(format!("Failed to read file: {e}"), None)
        })?;

        // Parse frontmatter
        let frontmatter = parse_yaml_frontmatter(&content).unwrap_or_else(|| serde_json::json!({}));

        // Get basic stats
        let word_count = content.split_whitespace().count();
        let char_count = content.chars().filter(|c| !c.is_whitespace()).count();
        let line_count = content.lines().count();

        // Count headings (lines starting with #)
        let heading_count = content
            .lines()
            .filter(|line| line.trim_start().starts_with('#'))
            .count();

        // Get file metadata
        let metadata = std::fs::metadata(&full_path).ok();
        let modified = metadata
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());

        Ok(CallToolResult::success(vec![rmcp::model::Content::json(
            serde_json::json!({
                "path": path,
                "frontmatter": frontmatter,
                "stats": {
                    "word_count": word_count,
                    "char_count": char_count,
                    "line_count": line_count,
                    "heading_count": heading_count,
                },
                "modified": modified
            }),
        )?]))
    }

    #[tool(description = "Update an existing note")]
    pub async fn update_note(
        &self,
        params: Parameters<UpdateNoteParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let params = params.0;
        let path = ensure_md_suffix(params.path);
        let new_content = params.content;
        let new_frontmatter = params.frontmatter;

        // Security: Validate path to prevent traversal attacks
        let full_path = validate_path_within_kiln(&self.kiln_path, &path)?;

        if !full_path.exists() {
            return Err(rmcp::ErrorData::invalid_params(
                format!("File not found: {path}"),
                None,
            ));
        }

        // Read existing file
        let existing_content = std::fs::read_to_string(&full_path).map_err(|e| {
            rmcp::ErrorData::internal_error(format!("Failed to read file: {e}"), None)
        })?;

        // Track what fields are being updated
        let mut updated_fields = Vec::new();

        // Determine the final frontmatter and content
        let (final_frontmatter, final_content) = match (new_frontmatter, new_content) {
            (Some(fm), Some(content)) => {
                // Update both frontmatter and content
                updated_fields.push("frontmatter");
                updated_fields.push("content");
                (Some(fm), content)
            }
            (Some(fm), None) => {
                // Update frontmatter only, preserve content
                updated_fields.push("frontmatter");
                let content = extract_content_without_frontmatter(&existing_content);
                (Some(fm), content)
            }
            (None, Some(content)) => {
                // Update content only, preserve frontmatter
                updated_fields.push("content");
                let fm = parse_yaml_frontmatter(&existing_content);
                (fm, content)
            }
            (None, None) => {
                // Nothing to update
                return Err(rmcp::ErrorData::invalid_params(
                    "Must provide either content or frontmatter to update".to_string(),
                    None,
                ));
            }
        };

        // Build final file content
        let final_file_content = if let Some(fm) = final_frontmatter {
            let fm_str = serialize_frontmatter_to_yaml(&fm)
                .map_err(|e| rmcp::ErrorData::internal_error(e, None))?;
            format!("{fm_str}{final_content}")
        } else {
            final_content
        };

        std::fs::write(&full_path, &final_file_content).map_err(|e| {
            rmcp::ErrorData::internal_error(format!("Failed to update file: {e}"), None)
        })?;

        // TODO: Notify parser for reprocessing

        Ok(CallToolResult::success(vec![rmcp::model::Content::json(
            serde_json::json!({
                "path": path,
                "full_path": full_path.to_string_lossy(),
                "status": "updated",
                "updated_fields": updated_fields
            }),
        )?]))
    }

    #[tool(description = "Delete a note from the kiln")]
    pub async fn delete_note(
        &self,
        params: Parameters<DeleteNoteParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let params = params.0;
        let path = ensure_md_suffix(params.path);

        // Security: Validate path to prevent traversal attacks
        let full_path = validate_path_within_kiln(&self.kiln_path, &path)?;

        if !full_path.exists() {
            return Err(rmcp::ErrorData::invalid_params(
                format!("File not found: {path}"),
                None,
            ));
        }

        std::fs::remove_file(&full_path).map_err(|e| {
            rmcp::ErrorData::internal_error(format!("Failed to delete file: {e}"), None)
        })?;

        // TODO: Notify parser for reprocessing

        Ok(CallToolResult::success(vec![rmcp::model::Content::json(
            serde_json::json!({
                "path": path,
                "full_path": full_path.to_string_lossy(),
                "status": "deleted"
            }),
        )?]))
    }

    #[tool(description = "List notes in a directory")]
    pub async fn list_notes(
        &self,
        params: Parameters<ListNotesParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let params = params.0;
        let folder = params.folder.clone();
        let include_frontmatter = params.include_frontmatter;
        let recursive = params.recursive;

        // Security: Validate folder to prevent traversal attacks
        let search_path = validate_folder_within_kiln(&self.kiln_path, folder.as_deref())?;

        if !search_path.exists() {
            return Err(rmcp::ErrorData::invalid_params(
                format!("Folder not found: {}", search_path.display()),
                None,
            ));
        }

        // Try NoteStore first for faster indexed access
        if let Some(ref note_store) = self.note_store {
            return self
                .list_notes_via_store(
                    note_store,
                    folder.as_deref(),
                    include_frontmatter,
                    recursive,
                )
                .await;
        }

        // Fallback: list from filesystem
        self.list_notes_via_filesystem(
            &search_path,
            folder.as_deref(),
            include_frontmatter,
            recursive,
        )
        .await
    }

    /// List notes using `NoteStore` index
    async fn list_notes_via_store(
        &self,
        note_store: &Arc<dyn NoteStore>,
        folder: Option<&str>,
        include_frontmatter: bool,
        recursive: bool,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let all_notes = note_store.list().await.map_err(|e| {
            rmcp::ErrorData::internal_error(format!("Failed to list notes from store: {e}"), None)
        })?;

        let folder_prefix = folder.unwrap_or("");
        let mut notes = Vec::new();

        for note in all_notes {
            // Filter by folder
            if !folder_prefix.is_empty() {
                if !note.path.starts_with(folder_prefix) {
                    continue;
                }

                // Non-recursive: check if note is in the immediate folder
                if !recursive {
                    let relative_to_folder =
                        note.path.strip_prefix(folder_prefix).unwrap_or(&note.path);
                    let relative_to_folder = relative_to_folder.trim_start_matches('/');
                    // If there's a / in the relative path, it's in a subfolder
                    if relative_to_folder.contains('/') {
                        continue;
                    }
                }
            } else if !recursive {
                // Non-recursive at root: only top-level files
                if note.path.contains('/') {
                    continue;
                }
            }

            let modified: Option<u64> = note.updated_at.timestamp().try_into().ok();

            let mut note_json = serde_json::json!({
                "path": note.path,
                "title": note.title,
                "modified": modified,
                "source": "index"
            });

            if include_frontmatter {
                // Build frontmatter from NoteRecord
                let mut frontmatter = serde_json::json!({
                    "title": note.title,
                    "tags": note.tags,
                });

                if let Some(obj) = frontmatter.as_object_mut() {
                    for (k, v) in &note.properties {
                        obj.insert(k.clone(), v.clone());
                    }
                }

                note_json["frontmatter"] = frontmatter;
                note_json["tags_count"] = serde_json::json!(note.tags.len());
                note_json["links_count"] = serde_json::json!(note.links_to.len());
            }

            notes.push(note_json);
        }

        Ok(CallToolResult::success(vec![rmcp::model::Content::json(
            serde_json::json!({
                "notes": notes,
                "folder": folder,
                "count": notes.len(),
                "recursive": recursive,
                "source": "index"
            }),
        )?]))
    }

    /// List notes using filesystem scanning (fallback)
    ///
    /// This function is async for API consistency with the `NoteStore` path,
    /// even though filesystem operations are synchronous.
    #[allow(clippy::unused_async)]
    async fn list_notes_via_filesystem(
        &self,
        search_path: &std::path::Path,
        folder: Option<&str>,
        include_frontmatter: bool,
        recursive: bool,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let mut notes = Vec::new();

        // Use WalkDir for recursive or std::fs::read_dir for non-recursive
        if recursive {
            for entry in WalkDir::new(search_path)
                .follow_links(false)
                .into_iter()
                .filter_map(std::result::Result::ok)
                .filter(|e| e.file_type().is_file())
                .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
            {
                let path = entry.path();
                if let Ok(relative_path) = path.strip_prefix(&self.kiln_path) {
                    let metadata = entry.metadata().ok();
                    let modified = metadata
                        .as_ref()
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs());

                    let mut note_json = serde_json::json!({
                        "path": relative_path.to_string_lossy(),
                        "size": metadata.as_ref().map_or(0, std::fs::Metadata::len),
                        "modified": modified
                    });

                    if include_frontmatter {
                        if let Ok(content) = std::fs::read_to_string(path) {
                            let frontmatter = parse_yaml_frontmatter(&content)
                                .unwrap_or_else(|| serde_json::json!({}));
                            note_json["frontmatter"] = frontmatter;
                            note_json["word_count"] =
                                serde_json::json!(content.split_whitespace().count());
                        }
                    }

                    notes.push(note_json);
                }
            }
        } else {
            // Non-recursive: just immediate children
            for entry in std::fs::read_dir(search_path).map_err(|e| {
                rmcp::ErrorData::internal_error(format!("Failed to read directory: {e}"), None)
            })? {
                let entry = entry.map_err(|e| {
                    rmcp::ErrorData::internal_error(format!("Failed to read entry: {e}"), None)
                })?;
                let path = entry.path();

                if path.is_file() && path.extension().is_some_and(|ext| ext == "md") {
                    if let Ok(relative_path) = path.strip_prefix(&self.kiln_path) {
                        let metadata = entry.metadata().ok();
                        let modified = metadata
                            .as_ref()
                            .and_then(|m| m.modified().ok())
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs());

                        let mut note_json = serde_json::json!({
                            "path": relative_path.to_string_lossy(),
                            "size": metadata.as_ref().map_or(0, std::fs::Metadata::len),
                            "modified": modified
                        });

                        if include_frontmatter {
                            if let Ok(content) = std::fs::read_to_string(&path) {
                                let frontmatter = parse_yaml_frontmatter(&content)
                                    .unwrap_or_else(|| serde_json::json!({}));
                                note_json["frontmatter"] = frontmatter;
                                note_json["word_count"] =
                                    serde_json::json!(content.split_whitespace().count());
                            }
                        }

                        notes.push(note_json);
                    }
                }
            }
        }

        Ok(CallToolResult::success(vec![rmcp::model::Content::json(
            serde_json::json!({
                "notes": notes,
                "folder": folder,
                "count": notes.len(),
                "recursive": recursive
            }),
        )?]))
    }
}

// Use shared utilities for frontmatter parsing and path validation
use crate::utils::{
    parse_yaml_frontmatter, validate_folder_within_kiln, validate_path_within_kiln,
};

/// Serialize frontmatter to YAML format with delimiters
fn serialize_frontmatter_to_yaml(frontmatter: &serde_json::Value) -> Result<String, String> {
    // If frontmatter is empty object, return empty string
    if let Some(obj) = frontmatter.as_object() {
        if obj.is_empty() {
            return Ok(String::new());
        }
    }

    // Serialize to YAML
    let yaml_str = serde_yaml::to_string(frontmatter)
        .map_err(|e| format!("Failed to serialize frontmatter: {e}"))?;

    // Add delimiters
    Ok(format!("---\n{yaml_str}---\n"))
}

/// Extract content without frontmatter
fn extract_content_without_frontmatter(content: &str) -> String {
    // Check if starts with ---
    if !content.starts_with("---\n") && !content.starts_with("---\r\n") {
        return content.to_string();
    }

    // Find closing ---
    let rest = &content[4..]; // Skip opening ---\n
    if let Some(end_pos) = rest.find("\n---\n") {
        // Return content after closing ---
        rest[end_pos + 5..].to_string()
    } else if let Some(end_pos) = rest.find("\r\n---\r\n") {
        rest[end_pos + 7..].to_string()
    } else {
        // No closing delimiter found, return original
        content.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_note_tools_creation() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let note_tools = NoteTools::new(kiln_path);
        assert_eq!(note_tools.kiln_path, temp_dir.path().to_string_lossy());
    }

    #[tokio::test]
    async fn test_create_note() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let note_tools = NoteTools::new(kiln_path);

        let result = note_tools
            .create_note(Parameters(CreateNoteParams {
                path: "test.md".to_string(),
                content: "# Test Note\n\nThis is a test note.".to_string(),
                frontmatter: None,
            }))
            .await;

        assert!(result.is_ok());

        let call_result = result.unwrap();
        assert!(!call_result.content.is_empty());

        // Check that the response is JSON with expected structure
        if let Some(content) = call_result.content.first() {
            if let Some(raw_text) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
                assert_eq!(parsed["path"], "test.md");
                assert_eq!(parsed["status"], "created");
                assert!(parsed["full_path"].as_str().unwrap().contains("test.md"));
            }
        }
    }

    #[tokio::test]
    async fn test_create_and_read_note() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let note_tools = NoteTools::new(kiln_path);
        let content = "# Test Note\n\nThis is a test note.";

        // Create note
        let create_result = note_tools
            .create_note(Parameters(CreateNoteParams {
                path: "test.md".to_string(),
                content: content.to_string(),
                frontmatter: None,
            }))
            .await;
        assert!(create_result.is_ok());

        // Read note
        let read_result = note_tools
            .read_note(Parameters(ReadNoteParams {
                path: "test.md".to_string(),
                start_line: None,
                end_line: None,
            }))
            .await;
        assert!(read_result.is_ok());

        let call_result = read_result.unwrap();

        // Verify the response structure and content
        if let Some(response_content) = call_result.content.first() {
            if let Some(raw_text) = response_content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
                assert_eq!(parsed["path"], "test.md");
                assert_eq!(parsed["content"], content);
                assert_eq!(parsed["total_lines"], 3);
            }
        }
    }

    #[tokio::test]
    async fn test_read_nonexistent_note() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let note_tools = NoteTools::new(kiln_path);

        let result = note_tools
            .read_note(Parameters(ReadNoteParams {
                path: "nonexistent.md".to_string(),
                start_line: None,
                end_line: None,
            }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_create_note_without_md_suffix() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let note_tools = NoteTools::new(kiln_path.clone());

        let result = note_tools
            .create_note(Parameters(CreateNoteParams {
                path: "wikilink".to_string(),
                content: "# Wiki\n".to_string(),
                frontmatter: None,
            }))
            .await;
        assert!(result.is_ok());

        let call_result = result.unwrap();
        if let Some(content) = call_result.content.first() {
            if let Some(raw_text) = content.as_text() {
                let parsed: serde_json::Value =
                    serde_json::from_str(&raw_text.text).expect("Valid JSON response");
                assert_eq!(parsed["path"], "wikilink.md");
            }
        }
        assert!(temp_dir.path().join("wikilink.md").exists());
    }

    #[tokio::test]
    async fn test_read_note_without_md_suffix() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let note_tools = NoteTools::new(kiln_path.clone());
        let note_path = temp_dir.path().join("wikilink.md");
        std::fs::write(&note_path, "content").unwrap();

        let result = note_tools
            .read_note(Parameters(ReadNoteParams {
                path: "wikilink".to_string(),
                start_line: None,
                end_line: None,
            }))
            .await;
        assert!(result.is_ok());

        let call_result = result.unwrap();
        if let Some(content) = call_result.content.first() {
            if let Some(raw_text) = content.as_text() {
                let parsed: serde_json::Value =
                    serde_json::from_str(&raw_text.text).expect("Valid JSON response");
                assert_eq!(parsed["path"], "wikilink.md");
                assert_eq!(parsed["content"], "content");
            }
        }
    }

    #[tokio::test]
    async fn test_update_note() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let note_tools = NoteTools::new(kiln_path);
        let initial_content = "# Initial Content";
        let updated_content = "# Updated Content\n\nWith more text.";

        // Create note first
        note_tools
            .create_note(Parameters(CreateNoteParams {
                path: "update.md".to_string(),
                content: initial_content.to_string(),
                frontmatter: None,
            }))
            .await
            .unwrap();

        // Update note
        let result = note_tools
            .update_note(Parameters(UpdateNoteParams {
                path: "update.md".to_string(),
                content: Some(updated_content.to_string()),
                frontmatter: None,
            }))
            .await;
        assert!(result.is_ok());

        // Verify update response structure
        if let Some(content) = result.unwrap().content.first() {
            if let Some(raw_text) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
                assert_eq!(parsed["path"], "update.md");
                assert_eq!(parsed["status"], "updated");
            }
        }

        // Verify file content
        let read_result = note_tools
            .read_note(Parameters(ReadNoteParams {
                path: "update.md".to_string(),
                start_line: None,
                end_line: None,
            }))
            .await
            .unwrap();
        if let Some(content) = read_result.content.first() {
            if let Some(raw_text) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
                assert_eq!(parsed["content"], updated_content);
            }
        }
    }

    #[tokio::test]
    async fn test_update_nonexistent_note() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let note_tools = NoteTools::new(kiln_path);

        let result = note_tools
            .update_note(Parameters(UpdateNoteParams {
                path: "nonexistent.md".to_string(),
                content: Some("content".to_string()),
                frontmatter: None,
            }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_delete_note() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let note_tools = NoteTools::new(kiln_path);

        // Create note first
        note_tools
            .create_note(Parameters(CreateNoteParams {
                path: "delete.md".to_string(),
                content: "content".to_string(),
                frontmatter: None,
            }))
            .await
            .unwrap();

        // Delete note
        let result = note_tools
            .delete_note(Parameters(DeleteNoteParams {
                path: "delete.md".to_string(),
            }))
            .await;
        assert!(result.is_ok());

        // Verify delete response structure
        if let Some(content) = result.unwrap().content.first() {
            if let Some(raw_text) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
                assert_eq!(parsed["path"], "delete.md");
                assert_eq!(parsed["status"], "deleted");
            }
        }

        // Verify deletion
        let read_result = note_tools
            .read_note(Parameters(ReadNoteParams {
                path: "delete.md".to_string(),
                start_line: None,
                end_line: None,
            }))
            .await;
        assert!(read_result.is_err());
    }

    #[tokio::test]
    async fn test_list_notes_empty() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let note_tools = NoteTools::new(kiln_path.clone());

        let result = note_tools
            .list_notes(Parameters(ListNotesParams {
                folder: None,
                include_frontmatter: false,
                recursive: true,
            }))
            .await;
        assert!(result.is_ok());

        let call_result = result.unwrap();
        if let Some(content) = call_result.content.first() {
            if let Some(raw_text) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
                assert_eq!(parsed["notes"].as_array().unwrap().len(), 0);
                assert_eq!(parsed["count"], 0);
            }
        }
    }

    #[tokio::test]
    async fn test_list_notes_with_files() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let note_tools = NoteTools::new(kiln_path);

        // Create some test files
        note_tools
            .create_note(Parameters(CreateNoteParams {
                path: "test1.md".to_string(),
                content: "content1".to_string(),
                frontmatter: None,
            }))
            .await
            .unwrap();
        note_tools
            .create_note(Parameters(CreateNoteParams {
                path: "test2.md".to_string(),
                content: "content2".to_string(),
                frontmatter: None,
            }))
            .await
            .unwrap();

        // Create a non-md file (should be ignored)
        std::fs::write(temp_dir.path().join("ignore.txt"), "ignore").unwrap();

        let result = note_tools
            .list_notes(Parameters(ListNotesParams {
                folder: None,
                include_frontmatter: false,
                recursive: true,
            }))
            .await;
        assert!(result.is_ok());

        let call_result = result.unwrap();
        if let Some(content) = call_result.content.first() {
            if let Some(raw_text) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
                let notes = parsed["notes"].as_array().unwrap();
                assert_eq!(notes.len(), 2); // Should only find .md files
                assert_eq!(parsed["count"], 2);

                // Check that all notes have required fields
                for note in notes {
                    assert!(note["path"].is_string());
                    assert!(note["size"].is_number());
                }
            }
        }
    }

    // ===== Phase 2: New tests for read_metadata and line range support =====

    #[tokio::test]
    async fn test_read_metadata_with_frontmatter() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();
        let note_tools = NoteTools::new(kiln_path);

        // Create note with frontmatter
        let content = "---\ntitle: Test Note\ntags: [test, important]\nstatus: draft\n---\n\n# Test Note\n\nSome content here.";
        note_tools
            .create_note(Parameters(CreateNoteParams {
                path: "test.md".to_string(),
                content: content.to_string(),
                frontmatter: None,
            }))
            .await
            .unwrap();

        // Read metadata
        let result = note_tools
            .read_metadata(Parameters(ReadMetadataParams {
                path: "test.md".to_string(),
            }))
            .await;
        assert!(result.is_ok());

        let call_result = result.unwrap();
        if let Some(response_content) = call_result.content.first() {
            if let Some(raw_text) = response_content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();

                // Check frontmatter
                assert_eq!(parsed["frontmatter"]["title"], "Test Note");
                assert_eq!(parsed["frontmatter"]["status"], "draft");
                assert_eq!(parsed["frontmatter"]["tags"].as_array().unwrap().len(), 2);

                // Check stats
                assert!(parsed["stats"]["word_count"].as_u64().unwrap() > 0);
                assert!(parsed["stats"]["heading_count"].as_u64().unwrap() == 1);
                assert!(parsed["modified"].is_number());
            }
        }
    }

    #[tokio::test]
    async fn test_read_metadata_without_frontmatter() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();
        let note_tools = NoteTools::new(kiln_path);

        // Create note without frontmatter
        let content = "# Test Note\n\nJust content, no frontmatter.";
        note_tools
            .create_note(Parameters(CreateNoteParams {
                path: "test.md".to_string(),
                content: content.to_string(),
                frontmatter: None,
            }))
            .await
            .unwrap();

        // Read metadata
        let result = note_tools
            .read_metadata(Parameters(ReadMetadataParams {
                path: "test.md".to_string(),
            }))
            .await;
        assert!(result.is_ok());

        let call_result = result.unwrap();
        if let Some(response_content) = call_result.content.first() {
            if let Some(raw_text) = response_content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();

                // Frontmatter should be empty object
                assert_eq!(parsed["frontmatter"], serde_json::json!({}));

                // Stats should still be present
                assert!(parsed["stats"]["word_count"].as_u64().unwrap() > 0);
            }
        }
    }

    #[tokio::test]
    async fn test_read_note_line_range_full() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();
        let note_tools = NoteTools::new(kiln_path);

        let content = "line 1\nline 2\nline 3\nline 4\nline 5";
        note_tools
            .create_note(Parameters(CreateNoteParams {
                path: "test.md".to_string(),
                content: content.to_string(),
                frontmatter: None,
            }))
            .await
            .unwrap();

        // Read full file (no line range)
        let result = note_tools
            .read_note(Parameters(ReadNoteParams {
                path: "test.md".to_string(),
                start_line: None,
                end_line: None,
            }))
            .await
            .unwrap();

        if let Some(response_content) = result.content.first() {
            if let Some(raw_text) = response_content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
                assert_eq!(parsed["total_lines"], 5);
                assert_eq!(parsed["lines_returned"], 5);
                assert_eq!(parsed["content"], content);
            }
        }
    }

    #[tokio::test]
    async fn test_read_note_first_n_lines() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();
        let note_tools = NoteTools::new(kiln_path);

        let content = "line 1\nline 2\nline 3\nline 4\nline 5";
        note_tools
            .create_note(Parameters(CreateNoteParams {
                path: "test.md".to_string(),
                content: content.to_string(),
                frontmatter: None,
            }))
            .await
            .unwrap();

        // Read first 3 lines
        let result = note_tools
            .read_note(Parameters(ReadNoteParams {
                path: "test.md".to_string(),
                start_line: None,
                end_line: Some(3),
            }))
            .await
            .unwrap();

        if let Some(response_content) = result.content.first() {
            if let Some(raw_text) = response_content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
                assert_eq!(parsed["total_lines"], 5);
                assert_eq!(parsed["lines_returned"], 3);
                assert_eq!(parsed["content"], "line 1\nline 2\nline 3");
            }
        }
    }

    #[tokio::test]
    async fn test_read_note_line_range() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();
        let note_tools = NoteTools::new(kiln_path);

        let content = "line 1\nline 2\nline 3\nline 4\nline 5";
        note_tools
            .create_note(Parameters(CreateNoteParams {
                path: "test.md".to_string(),
                content: content.to_string(),
                frontmatter: None,
            }))
            .await
            .unwrap();

        // Read lines 2-4 (1-indexed)
        let result = note_tools
            .read_note(Parameters(ReadNoteParams {
                path: "test.md".to_string(),
                start_line: Some(2),
                end_line: Some(4),
            }))
            .await
            .unwrap();

        if let Some(response_content) = result.content.first() {
            if let Some(raw_text) = response_content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
                assert_eq!(parsed["total_lines"], 5);
                assert_eq!(parsed["lines_returned"], 3);
                assert_eq!(parsed["content"], "line 2\nline 3\nline 4");
            }
        }
    }

    #[tokio::test]
    async fn test_read_note_from_start_line() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();
        let note_tools = NoteTools::new(kiln_path);

        let content = "line 1\nline 2\nline 3\nline 4\nline 5";
        note_tools
            .create_note(Parameters(CreateNoteParams {
                path: "test.md".to_string(),
                content: content.to_string(),
                frontmatter: None,
            }))
            .await
            .unwrap();

        // Read from line 3 to end
        let result = note_tools
            .read_note(Parameters(ReadNoteParams {
                path: "test.md".to_string(),
                start_line: Some(3),
                end_line: None,
            }))
            .await
            .unwrap();

        if let Some(response_content) = result.content.first() {
            if let Some(raw_text) = response_content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
                assert_eq!(parsed["total_lines"], 5);
                assert_eq!(parsed["lines_returned"], 3);
                assert_eq!(parsed["content"], "line 3\nline 4\nline 5");
            }
        }
    }

    #[tokio::test]
    async fn test_list_notes_with_frontmatter() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();
        let note_tools = NoteTools::new(kiln_path);

        // Create notes with frontmatter
        note_tools
            .create_note(Parameters(CreateNoteParams {
                path: "note1.md".to_string(),
                content: "---\ntitle: Note 1\nstatus: draft\n---\n\nContent".to_string(),
                frontmatter: None,
            }))
            .await
            .unwrap();

        note_tools
            .create_note(Parameters(CreateNoteParams {
                path: "note2.md".to_string(),
                content: "---\ntitle: Note 2\nstatus: published\n---\n\nContent".to_string(),
                frontmatter: None,
            }))
            .await
            .unwrap();

        // List with frontmatter
        let result = note_tools
            .list_notes(Parameters(ListNotesParams {
                folder: None,
                include_frontmatter: true,
                recursive: true,
            }))
            .await
            .unwrap();

        if let Some(response_content) = result.content.first() {
            if let Some(raw_text) = response_content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
                let notes = parsed["notes"].as_array().unwrap();
                assert_eq!(notes.len(), 2);

                // Check that frontmatter is included
                for note in notes {
                    assert!(note["frontmatter"].is_object());
                    assert!(note["frontmatter"]["title"].is_string());
                    assert!(note["word_count"].is_number());
                }
            }
        }
    }

    #[tokio::test]
    async fn test_list_notes_non_recursive() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();
        let note_tools = NoteTools::new(kiln_path);

        // Create root note
        note_tools
            .create_note(Parameters(CreateNoteParams {
                path: "root.md".to_string(),
                content: "Root note".to_string(),
                frontmatter: None,
            }))
            .await
            .unwrap();

        // Create subfolder with note
        std::fs::create_dir(temp_dir.path().join("subfolder")).unwrap();
        note_tools
            .create_note(Parameters(CreateNoteParams {
                path: "subfolder/nested.md".to_string(),
                content: "Nested note".to_string(),
                frontmatter: None,
            }))
            .await
            .unwrap();

        // List non-recursively (should only find root.md)
        let result = note_tools
            .list_notes(Parameters(ListNotesParams {
                folder: None,
                include_frontmatter: false,
                recursive: false,
            }))
            .await
            .unwrap();

        if let Some(response_content) = result.content.first() {
            if let Some(raw_text) = response_content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
                assert_eq!(parsed["count"], 1);
                assert_eq!(parsed["recursive"], false);
            }
        }
    }

    #[tokio::test]
    async fn test_create_note_with_frontmatter() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();
        let note_tools = NoteTools::new(kiln_path.clone());

        let frontmatter = serde_json::json!({
            "title": "Test Note",
            "tags": ["test", "example"],
            "status": "draft"
        });

        let result = note_tools
            .create_note(Parameters(CreateNoteParams {
                path: "test_frontmatter.md".to_string(),
                content: "This is the content".to_string(),
                frontmatter: Some(frontmatter.clone()),
            }))
            .await
            .unwrap();

        if let Some(response_content) = result.content.first() {
            if let Some(raw_text) = response_content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
                assert_eq!(parsed["status"], "created");
            }
        }

        // Verify file content has frontmatter
        let file_path = temp_dir.path().join("test_frontmatter.md");
        let content = std::fs::read_to_string(file_path).unwrap();

        assert!(content.starts_with("---\n"));
        assert!(content.contains("title: Test Note"));
        assert!(content.contains("tags:"));
        assert!(content.contains("- test"));
        assert!(content.contains("- example"));
        assert!(content.contains("status: draft"));
        assert!(content.contains("---\nThis is the content"));
    }

    #[tokio::test]
    async fn test_create_note_without_frontmatter() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();
        let note_tools = NoteTools::new(kiln_path.clone());

        let result = note_tools
            .create_note(Parameters(CreateNoteParams {
                path: "test_no_frontmatter.md".to_string(),
                content: "Just content".to_string(),
                frontmatter: None,
            }))
            .await
            .unwrap();

        if let Some(response_content) = result.content.first() {
            if let Some(raw_text) = response_content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
                assert_eq!(parsed["status"], "created");
            }
        }

        // Verify file content has NO frontmatter
        let file_path = temp_dir.path().join("test_no_frontmatter.md");
        let content = std::fs::read_to_string(file_path).unwrap();

        assert_eq!(content, "Just content");
    }

    #[tokio::test]
    async fn test_update_note_content_only() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();
        let note_tools = NoteTools::new(kiln_path.clone());

        // Create initial note with frontmatter
        let initial_frontmatter = serde_json::json!({
            "title": "Original",
            "tags": ["original"]
        });

        note_tools
            .create_note(Parameters(CreateNoteParams {
                path: "update_test.md".to_string(),
                content: "Original content".to_string(),
                frontmatter: Some(initial_frontmatter),
            }))
            .await
            .unwrap();

        // Update content only (frontmatter should remain)
        let result = note_tools
            .update_note(Parameters(UpdateNoteParams {
                path: "update_test.md".to_string(),
                content: Some("New content".to_string()),
                frontmatter: None,
            }))
            .await
            .unwrap();

        if let Some(response_content) = result.content.first() {
            if let Some(raw_text) = response_content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
                assert_eq!(parsed["status"], "updated");
                assert_eq!(parsed["updated_fields"], serde_json::json!(["content"]));
            }
        }

        // Verify frontmatter preserved, content updated
        let file_path = temp_dir.path().join("update_test.md");
        let content = std::fs::read_to_string(file_path).unwrap();

        assert!(content.contains("title: Original"));
        assert!(content.contains("- original"));
        assert!(content.contains("---\nNew content"));
    }

    #[tokio::test]
    async fn test_update_note_frontmatter_only() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();
        let note_tools = NoteTools::new(kiln_path.clone());

        // Create initial note
        note_tools
            .create_note(Parameters(CreateNoteParams {
                path: "update_fm_test.md".to_string(),
                content: "Original content".to_string(),
                frontmatter: Some(serde_json::json!({"title": "Original"})),
            }))
            .await
            .unwrap();

        // Update frontmatter only (content should remain)
        let new_frontmatter = serde_json::json!({
            "title": "Updated",
            "tags": ["new"]
        });

        let result = note_tools
            .update_note(Parameters(UpdateNoteParams {
                path: "update_fm_test.md".to_string(),
                content: None,
                frontmatter: Some(new_frontmatter),
            }))
            .await
            .unwrap();

        if let Some(response_content) = result.content.first() {
            if let Some(raw_text) = response_content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
                assert_eq!(parsed["status"], "updated");
                assert_eq!(parsed["updated_fields"], serde_json::json!(["frontmatter"]));
            }
        }

        // Verify frontmatter updated, content preserved
        let file_path = temp_dir.path().join("update_fm_test.md");
        let content = std::fs::read_to_string(file_path).unwrap();

        assert!(content.contains("title: Updated"));
        assert!(content.contains("- new"));
        assert!(content.contains("---\nOriginal content"));
    }

    #[tokio::test]
    async fn test_update_note_both_content_and_frontmatter() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();
        let note_tools = NoteTools::new(kiln_path.clone());

        // Create initial note
        note_tools
            .create_note(Parameters(CreateNoteParams {
                path: "update_both_test.md".to_string(),
                content: "Original content".to_string(),
                frontmatter: Some(serde_json::json!({"title": "Original"})),
            }))
            .await
            .unwrap();

        // Update both
        let result = note_tools
            .update_note(Parameters(UpdateNoteParams {
                path: "update_both_test.md".to_string(),
                content: Some("New content".to_string()),
                frontmatter: Some(serde_json::json!({"title": "New", "status": "published"})),
            }))
            .await
            .unwrap();

        if let Some(response_content) = result.content.first() {
            if let Some(raw_text) = response_content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
                assert_eq!(parsed["status"], "updated");
                // Should contain both fields
                let updated_fields = parsed["updated_fields"].as_array().unwrap();
                assert!(updated_fields.contains(&serde_json::json!("content")));
                assert!(updated_fields.contains(&serde_json::json!("frontmatter")));
            }
        }

        // Verify both updated
        let file_path = temp_dir.path().join("update_both_test.md");
        let content = std::fs::read_to_string(file_path).unwrap();

        assert!(content.contains("title: New"));
        assert!(content.contains("status: published"));
        assert!(content.contains("---\nNew content"));
    }

    #[tokio::test]
    async fn test_update_note_remove_frontmatter() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();
        let note_tools = NoteTools::new(kiln_path.clone());

        // Create note with frontmatter
        note_tools
            .create_note(Parameters(CreateNoteParams {
                path: "remove_fm_test.md".to_string(),
                content: "Content".to_string(),
                frontmatter: Some(serde_json::json!({"title": "Test"})),
            }))
            .await
            .unwrap();

        // Update with content only and empty frontmatter to remove it
        let result = note_tools
            .update_note(Parameters(UpdateNoteParams {
                path: "remove_fm_test.md".to_string(),
                content: Some("Just content".to_string()),
                frontmatter: Some(serde_json::json!({})),
            }))
            .await
            .unwrap();

        if let Some(response_content) = result.content.first() {
            if let Some(raw_text) = response_content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
                assert_eq!(parsed["status"], "updated");
            }
        }

        // Verify frontmatter removed
        let file_path = temp_dir.path().join("remove_fm_test.md");
        let content = std::fs::read_to_string(file_path).unwrap();

        // Should be just content, no frontmatter block
        assert_eq!(content, "Just content");
    }

    #[test]
    fn test_tool_router_creation() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let _note_tools = NoteTools::new(kiln_path);

        // This should compile and not panic - the tool_router macro generates the router
        let _router = NoteTools::tool_router();
    }

    // ===== Security Tests for Path Traversal =====

    #[tokio::test]
    async fn test_create_note_path_traversal_parent_dir() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();
        let note_tools = NoteTools::new(kiln_path);

        let result = note_tools
            .create_note(Parameters(CreateNoteParams {
                path: "../../../etc/passwd".to_string(),
                content: "malicious content".to_string(),
                frontmatter: None,
            }))
            .await;

        assert!(result.is_err(), "Should reject path traversal attack");
        if let Err(e) = result {
            assert!(
                e.message.contains("Path traversal"),
                "Error should mention path traversal"
            );
        }
    }

    #[tokio::test]
    async fn test_create_note_path_traversal_absolute() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();
        let note_tools = NoteTools::new(kiln_path);

        let result = note_tools
            .create_note(Parameters(CreateNoteParams {
                path: "/etc/passwd".to_string(),
                content: "malicious content".to_string(),
                frontmatter: None,
            }))
            .await;

        assert!(result.is_err(), "Should reject absolute path");
        if let Err(e) = result {
            assert!(
                e.message.contains("Absolute paths are not allowed"),
                "Error should mention absolute paths"
            );
        }
    }

    #[tokio::test]
    async fn test_read_note_path_traversal() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();
        let note_tools = NoteTools::new(kiln_path);

        let result = note_tools
            .read_note(Parameters(ReadNoteParams {
                path: "../../etc/passwd".to_string(),
                start_line: None,
                end_line: None,
            }))
            .await;

        assert!(result.is_err(), "Should reject path traversal");
    }

    #[tokio::test]
    async fn test_update_note_path_traversal() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();
        let note_tools = NoteTools::new(kiln_path);

        let result = note_tools
            .update_note(Parameters(UpdateNoteParams {
                path: "../../../etc/passwd".to_string(),
                content: Some("malicious".to_string()),
                frontmatter: None,
            }))
            .await;

        assert!(result.is_err(), "Should reject path traversal");
    }

    #[tokio::test]
    async fn test_delete_note_path_traversal() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();
        let note_tools = NoteTools::new(kiln_path);

        let result = note_tools
            .delete_note(Parameters(DeleteNoteParams {
                path: "../../etc/passwd".to_string(),
            }))
            .await;

        assert!(result.is_err(), "Should reject path traversal");
    }

    #[tokio::test]
    async fn test_list_notes_path_traversal() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();
        let note_tools = NoteTools::new(kiln_path);

        let result = note_tools
            .list_notes(Parameters(ListNotesParams {
                folder: Some("../../../etc".to_string()),
                include_frontmatter: false,
                recursive: false,
            }))
            .await;

        assert!(result.is_err(), "Should reject path traversal in folder");
    }

    #[tokio::test]
    async fn test_read_metadata_path_traversal() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();
        let note_tools = NoteTools::new(kiln_path);

        let result = note_tools
            .read_metadata(Parameters(ReadMetadataParams {
                path: "../../../etc/passwd".to_string(),
            }))
            .await;

        assert!(result.is_err(), "Should reject path traversal");
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_symlink_escape_blocked() {
        use std::os::unix::fs::symlink;

        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();
        let note_tools = NoteTools::new(kiln_path.clone());

        // Create a directory outside the kiln
        let outside_dir = TempDir::new().unwrap();
        std::fs::write(outside_dir.path().join("secret.txt"), "secret data").unwrap();

        // Create a symlink inside kiln that points outside
        let symlink_path = temp_dir.path().join("evil_link");
        symlink(outside_dir.path(), &symlink_path).unwrap();

        // Try to create a file through the symlink
        let result = note_tools
            .create_note(Parameters(CreateNoteParams {
                path: "evil_link/secret.txt".to_string(),
                content: "overwrite attempt".to_string(),
                frontmatter: None,
            }))
            .await;

        assert!(
            result.is_err(),
            "Should reject symlink escape to outside kiln"
        );
    }

    #[tokio::test]
    async fn test_valid_nested_path_allowed() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();
        let note_tools = NoteTools::new(kiln_path);

        // Create nested directory
        std::fs::create_dir_all(temp_dir.path().join("projects/rust")).unwrap();

        // This should succeed - normal nested path
        let result = note_tools
            .create_note(Parameters(CreateNoteParams {
                path: "projects/rust/main.md".to_string(),
                content: "# Rust Project".to_string(),
                frontmatter: None,
            }))
            .await;

        assert!(result.is_ok(), "Should allow valid nested path");
    }
}

// ===== NoteStore Integration Tests =====
// These tests verify the NoteStore code path works correctly

#[cfg(test)]
mod note_store_tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::Utc;
    use crucible_core::parser::BlockHash;
    use crucible_core::storage::{Filter, NoteRecord, StorageResult};
    use std::collections::HashMap;
    use std::sync::Mutex;
    use tempfile::TempDir;

    /// Mock `NoteStore` for testing the `NoteStore` integration path
    struct MockNoteStore {
        notes: Mutex<HashMap<String, NoteRecord>>,
    }

    impl MockNoteStore {
        fn new() -> Self {
            Self {
                notes: Mutex::new(HashMap::new()),
            }
        }

        fn add_note(&self, record: NoteRecord) {
            let mut notes = self.notes.lock().unwrap();
            notes.insert(record.path.clone(), record);
        }
    }

    #[async_trait]
    impl NoteStore for MockNoteStore {
        async fn upsert(&self, note: NoteRecord) -> StorageResult<()> {
            self.add_note(note);
            Ok(())
        }

        async fn get(&self, path: &str) -> StorageResult<Option<NoteRecord>> {
            let notes = self.notes.lock().unwrap();
            Ok(notes.get(path).cloned())
        }

        async fn delete(&self, path: &str) -> StorageResult<()> {
            let mut notes = self.notes.lock().unwrap();
            notes.remove(path);
            Ok(())
        }

        async fn list(&self) -> StorageResult<Vec<NoteRecord>> {
            let notes = self.notes.lock().unwrap();
            Ok(notes.values().cloned().collect())
        }

        async fn get_by_hash(&self, _hash: &BlockHash) -> StorageResult<Option<NoteRecord>> {
            Ok(None)
        }

        async fn search(
            &self,
            _embedding: &[f32],
            _k: usize,
            _filter: Option<Filter>,
        ) -> StorageResult<Vec<crucible_core::storage::note_store::SearchResult>> {
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn test_read_metadata_uses_note_store() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // Create a mock NoteStore with a pre-populated note
        let mock_store = Arc::new(MockNoteStore::new());

        let mut properties = HashMap::new();
        properties.insert("status".to_string(), serde_json::json!("published"));

        mock_store.add_note(NoteRecord {
            path: "test.md".to_string(),
            content_hash: BlockHash::zero(),
            embedding: Some(vec![0.1; 384]),
            title: "Test Note from Index".to_string(),
            tags: vec!["rust".to_string(), "test".to_string()],
            links_to: vec!["other.md".to_string()],
            properties,
            updated_at: Utc::now(),
        });

        // Create NoteTools with the mock store
        let note_tools = NoteTools::with_note_store(kiln_path, mock_store);

        // Read metadata - should use the NoteStore, not filesystem
        let result = note_tools
            .read_metadata(Parameters(ReadMetadataParams {
                path: "test.md".to_string(),
            }))
            .await;

        assert!(result.is_ok(), "read_metadata should succeed");

        let call_result = result.unwrap();
        if let Some(content) = call_result.content.first() {
            if let Some(raw_text) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();

                // Verify data came from NoteStore (has "source": "index")
                assert_eq!(parsed["source"], "index", "Should indicate index source");
                assert_eq!(parsed["frontmatter"]["title"], "Test Note from Index");
                assert_eq!(parsed["frontmatter"]["status"], "published");

                // Verify stats from NoteStore
                assert_eq!(parsed["stats"]["links_count"], 1);
                assert_eq!(parsed["stats"]["tags_count"], 2);
                assert_eq!(parsed["stats"]["has_embedding"], true);
            }
        }
    }

    #[tokio::test]
    async fn test_read_metadata_fallback_to_filesystem() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // Create a mock NoteStore that doesn't have the note
        let mock_store = Arc::new(MockNoteStore::new());

        // Create a file on the filesystem
        let note_content = "---\ntitle: Filesystem Note\ntags: [fs]\n---\n\n# Content";
        std::fs::write(temp_dir.path().join("fs-note.md"), note_content).unwrap();

        // Create NoteTools with the mock store
        let note_tools = NoteTools::with_note_store(kiln_path, mock_store);

        // Read metadata - should fall back to filesystem since note not in store
        let result = note_tools
            .read_metadata(Parameters(ReadMetadataParams {
                path: "fs-note.md".to_string(),
            }))
            .await;

        assert!(result.is_ok(), "read_metadata should succeed via fallback");

        let call_result = result.unwrap();
        if let Some(content) = call_result.content.first() {
            if let Some(raw_text) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();

                // Should NOT have "source": "index" since it came from filesystem
                assert!(
                    parsed.get("source").is_none(),
                    "Should not have index source"
                );
                assert_eq!(parsed["frontmatter"]["title"], "Filesystem Note");
            }
        }
    }

    #[tokio::test]
    async fn test_list_notes_uses_note_store() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // Create a mock NoteStore with multiple notes
        let mock_store = Arc::new(MockNoteStore::new());

        mock_store.add_note(NoteRecord {
            path: "note1.md".to_string(),
            content_hash: BlockHash::zero(),
            embedding: None,
            title: "Note 1".to_string(),
            tags: vec!["tag1".to_string()],
            links_to: vec![],
            properties: HashMap::new(),
            updated_at: Utc::now(),
        });

        mock_store.add_note(NoteRecord {
            path: "folder/note2.md".to_string(),
            content_hash: BlockHash::zero(),
            embedding: None,
            title: "Note 2".to_string(),
            tags: vec!["tag2".to_string()],
            links_to: vec![],
            properties: HashMap::new(),
            updated_at: Utc::now(),
        });

        // Create NoteTools with the mock store
        let note_tools = NoteTools::with_note_store(kiln_path, mock_store);

        // List notes - should use the NoteStore
        let result = note_tools
            .list_notes(Parameters(ListNotesParams {
                folder: None,
                include_frontmatter: true,
                recursive: true,
            }))
            .await;

        assert!(result.is_ok(), "list_notes should succeed");

        let call_result = result.unwrap();
        if let Some(content) = call_result.content.first() {
            if let Some(raw_text) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();

                // Verify data came from NoteStore
                assert_eq!(parsed["source"], "index", "Should indicate index source");
                assert_eq!(parsed["count"], 2);

                let notes = parsed["notes"].as_array().unwrap();
                assert_eq!(notes.len(), 2);
            }
        }
    }

    #[tokio::test]
    async fn test_list_notes_filters_by_folder() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // Create a mock NoteStore with notes in different folders
        let mock_store = Arc::new(MockNoteStore::new());

        mock_store.add_note(NoteRecord {
            path: "root.md".to_string(),
            content_hash: BlockHash::zero(),
            embedding: None,
            title: "Root Note".to_string(),
            tags: vec![],
            links_to: vec![],
            properties: HashMap::new(),
            updated_at: Utc::now(),
        });

        mock_store.add_note(NoteRecord {
            path: "projects/rust.md".to_string(),
            content_hash: BlockHash::zero(),
            embedding: None,
            title: "Rust Project".to_string(),
            tags: vec![],
            links_to: vec![],
            properties: HashMap::new(),
            updated_at: Utc::now(),
        });

        mock_store.add_note(NoteRecord {
            path: "projects/python.md".to_string(),
            content_hash: BlockHash::zero(),
            embedding: None,
            title: "Python Project".to_string(),
            tags: vec![],
            links_to: vec![],
            properties: HashMap::new(),
            updated_at: Utc::now(),
        });

        // Create the folder on filesystem (required for validation)
        std::fs::create_dir_all(temp_dir.path().join("projects")).unwrap();

        // Create NoteTools with the mock store
        let note_tools = NoteTools::with_note_store(kiln_path, mock_store);

        // List notes in projects folder only
        let result = note_tools
            .list_notes(Parameters(ListNotesParams {
                folder: Some("projects".to_string()),
                include_frontmatter: false,
                recursive: true,
            }))
            .await;

        assert!(result.is_ok(), "list_notes should succeed");

        let call_result = result.unwrap();
        if let Some(content) = call_result.content.first() {
            if let Some(raw_text) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();

                // Should only return notes in the projects folder
                assert_eq!(parsed["count"], 2);
                assert_eq!(parsed["folder"], "projects");
            }
        }
    }

    #[tokio::test]
    async fn test_list_notes_non_recursive_with_store() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // Create a mock NoteStore with notes at different levels
        let mock_store = Arc::new(MockNoteStore::new());

        mock_store.add_note(NoteRecord {
            path: "root.md".to_string(),
            content_hash: BlockHash::zero(),
            embedding: None,
            title: "Root".to_string(),
            tags: vec![],
            links_to: vec![],
            properties: HashMap::new(),
            updated_at: Utc::now(),
        });

        mock_store.add_note(NoteRecord {
            path: "nested/deep.md".to_string(),
            content_hash: BlockHash::zero(),
            embedding: None,
            title: "Deep".to_string(),
            tags: vec![],
            links_to: vec![],
            properties: HashMap::new(),
            updated_at: Utc::now(),
        });

        let note_tools = NoteTools::with_note_store(kiln_path, mock_store);

        // List notes non-recursively at root
        let result = note_tools
            .list_notes(Parameters(ListNotesParams {
                folder: None,
                include_frontmatter: false,
                recursive: false,
            }))
            .await;

        assert!(result.is_ok());

        let call_result = result.unwrap();
        if let Some(content) = call_result.content.first() {
            if let Some(raw_text) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();

                // Non-recursive should only return root.md
                assert_eq!(parsed["count"], 1);
                assert_eq!(parsed["recursive"], false);
            }
        }
    }
}
