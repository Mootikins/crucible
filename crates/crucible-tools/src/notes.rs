//! Note CRUD operations tools
//!
//! This module provides simple filesystem-based note CRUD tools.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{model::CallToolResult, tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Clone)]
#[allow(missing_docs)]
pub struct NoteTools {
    kiln_path: String,
}

fn ensure_md_suffix(path: String) -> String {
    let pb = Path::new(&path);
    if pb.extension().is_some() {
        path
    } else {
        format!("{}.md", path)
    }
}

/// Parameters for creating a note
#[derive(Deserialize, JsonSchema)]
pub struct CreateNoteParams {
    path: String,
    content: String,
    #[serde(default)]
    frontmatter: Option<serde_json::Value>,
}

/// Parameters for reading a note
#[derive(Deserialize, JsonSchema)]
pub struct ReadNoteParams {
    path: String,
    #[serde(default)]
    start_line: Option<usize>,
    #[serde(default)]
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
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
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
    #[serde(default)]
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
    pub fn new(kiln_path: String) -> Self {
        Self { kiln_path }
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

        let full_path = std::path::Path::new(&self.kiln_path).join(&path);

        // Build final content with optional frontmatter
        let final_content = if let Some(fm) = frontmatter {
            let fm_str = serialize_frontmatter_to_yaml(&fm)
                .map_err(|e| rmcp::ErrorData::internal_error(e, None))?;
            format!("{}{}", fm_str, content)
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
        let full_path = std::path::Path::new(&self.kiln_path).join(&path);

        if !full_path.exists() {
            return Err(rmcp::ErrorData::invalid_params(
                format!("File not found: {}", path),
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
        let full_path = std::path::Path::new(&self.kiln_path).join(&path);

        if !full_path.exists() {
            return Err(rmcp::ErrorData::invalid_params(
                format!("File not found: {}", path),
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
        let full_path = std::path::Path::new(&self.kiln_path).join(&path);

        if !full_path.exists() {
            return Err(rmcp::ErrorData::invalid_params(
                format!("File not found: {}", path),
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
            format!("{}{}", fm_str, final_content)
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
        let full_path = std::path::Path::new(&self.kiln_path).join(&path);

        if !full_path.exists() {
            return Err(rmcp::ErrorData::invalid_params(
                format!("File not found: {}", path),
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
        let folder = params.folder;
        let include_frontmatter = params.include_frontmatter;
        let recursive = params.recursive;

        let search_path = if let Some(ref folder) = folder {
            std::path::Path::new(&self.kiln_path).join(folder)
        } else {
            std::path::Path::new(&self.kiln_path).to_path_buf()
        };

        if !search_path.exists() {
            return Err(rmcp::ErrorData::invalid_params(
                format!("Folder not found: {}", search_path.display()),
                None,
            ));
        }

        let mut notes = Vec::new();

        // Use WalkDir for recursive or std::fs::read_dir for non-recursive
        if recursive {
            for entry in WalkDir::new(&search_path)
                .follow_links(false)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
                .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
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
                        "size": metadata.as_ref().map(|m| m.len()).unwrap_or(0),
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
            for entry in std::fs::read_dir(&search_path).map_err(|e| {
                rmcp::ErrorData::internal_error(format!("Failed to read directory: {e}"), None)
            })? {
                let entry = entry.map_err(|e| {
                    rmcp::ErrorData::internal_error(format!("Failed to read entry: {e}"), None)
                })?;
                let path = entry.path();

                if path.is_file() && path.extension().map_or(false, |ext| ext == "md") {
                    if let Ok(relative_path) = path.strip_prefix(&self.kiln_path) {
                        let metadata = entry.metadata().ok();
                        let modified = metadata
                            .as_ref()
                            .and_then(|m| m.modified().ok())
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs());

                        let mut note_json = serde_json::json!({
                            "path": relative_path.to_string_lossy(),
                            "size": metadata.as_ref().map(|m| m.len()).unwrap_or(0),
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

// Use shared utility for frontmatter parsing
use crate::utils::parse_yaml_frontmatter;

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
        .map_err(|e| format!("Failed to serialize frontmatter: {}", e))?;

    // Add delimiters
    Ok(format!("---\n{}---\n", yaml_str))
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
}
