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

mod helpers;
mod list;
mod params;

#[cfg(test)]
mod tests;

use super::helpers::{json_success, McpResultExt};
use super::utils::{
    parse_yaml_frontmatter, validate_folder_within_kiln, validate_path_within_kiln,
};
use crucible_core::storage::NoteStore;
use helpers::{
    ensure_md_suffix, extract_content_without_frontmatter, serialize_frontmatter_to_yaml,
};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{model::CallToolResult, tool, tool_router};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use params::{
    CreateNoteParams, DeleteNoteParams, ListNotesParams, ReadMetadataParams, ReadNoteParams,
    UpdateNoteParams,
};

#[derive(Clone)]
#[allow(missing_docs)]
pub struct NoteTools {
    kiln_path: String,
    /// Optional `NoteStore` for faster metadata reads
    note_store: Option<Arc<dyn NoteStore>>,
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

    fn resolve_note_name(&self, path: &str) -> Result<String, rmcp::ErrorData> {
        if path.contains('/') || path.contains('\\') {
            return Ok(path.to_string());
        }

        let kiln_path = Path::new(&self.kiln_path);
        let resolved = self.find_note_by_name(kiln_path, path).ok_or_else(|| {
            rmcp::ErrorData::invalid_params(format!("File not found: {path}"), None)
        })?;

        let relative = resolved.strip_prefix(kiln_path).map_err(|_| {
            rmcp::ErrorData::invalid_params(
                format!("Resolved path escapes kiln directory: {path}"),
                None,
            )
        })?;

        Ok(relative.to_string_lossy().to_string())
    }

    fn find_note_by_name(&self, kiln_path: &Path, name: &str) -> Option<PathBuf> {
        let direct_path = kiln_path.join(name);
        if direct_path.is_file() {
            return Some(direct_path);
        }

        let mut stack = vec![kiln_path.to_path_buf()];
        while let Some(dir) = stack.pop() {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        stack.push(path);
                    } else if path
                        .file_name()
                        .and_then(|file_name| file_name.to_str())
                        .is_some_and(|file_name| file_name == name)
                    {
                        return Some(path);
                    }
                }
            }
        }

        None
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
            let fm_str = serialize_frontmatter_to_yaml(&fm).mcp_err()?;
            format!("{fm_str}{content}")
        } else {
            content
        };

        std::fs::write(&full_path, &final_content).mcp_err_ctx("Failed to write file")?;

        // TODO: Trigger re-parsing via crucible_core::parser after note creation

        json_success(serde_json::json!({
            "path": path,
            "status": "created"
        }))
    }

    #[tool(description = "Read note content with optional line range")]
    pub async fn read_note(
        &self,
        params: Parameters<ReadNoteParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let params = params.0;
        let path = ensure_md_suffix(params.path);
        let resolved_path = self.resolve_note_name(&path)?;

        // Security: Validate path to prevent traversal attacks
        let full_path = validate_path_within_kiln(&self.kiln_path, &resolved_path)?;

        if !full_path.exists() {
            return Err(rmcp::ErrorData::invalid_params(
                format!("File not found: {path}"),
                None,
            ));
        }

        let content = std::fs::read_to_string(&full_path).mcp_err_ctx("Failed to read file")?;

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

        json_success(serde_json::json!({
            "path": path,
            "content": content_slice,
            "total_lines": total_lines,
            "lines_returned": lines_returned
        }))
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
            // Workspace authority derived from this MCP server's bound kiln.
            let authority = crucible_core::storage::Scope::workspace(&self.kiln_path)
                .unwrap_or_else(|_| {
                    crucible_core::storage::Scope::workspace_unchecked(&self.kiln_path)
                });
            if let Ok(Some(note_record)) = note_store.get(&path, &authority).await {
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

                return json_success(serde_json::json!({
                    "path": path,
                    "frontmatter": frontmatter,
                    "stats": {
                        "links_count": note_record.links_to.len(),
                        "tags_count": note_record.tags.len(),
                        "has_embedding": note_record.has_embedding(),
                    },
                    "modified": modified,
                    "source": "index"
                }));
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

        let content = std::fs::read_to_string(&full_path).mcp_err_ctx("Failed to read file")?;

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

        json_success(serde_json::json!({
            "path": path,
            "frontmatter": frontmatter,
            "stats": {
                "word_count": word_count,
                "char_count": char_count,
                "line_count": line_count,
                "heading_count": heading_count,
            },
            "modified": modified
        }))
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
        let existing_content =
            std::fs::read_to_string(&full_path).mcp_err_ctx("Failed to read file")?;

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
            let fm_str = serialize_frontmatter_to_yaml(&fm).mcp_err()?;
            format!("{fm_str}{final_content}")
        } else {
            final_content
        };

        std::fs::write(&full_path, &final_file_content).mcp_err_ctx("Failed to update file")?;

        // TODO: Trigger re-parsing via crucible_core::parser after note update

        json_success(serde_json::json!({
            "path": path,
            "status": "updated",
            "updated_fields": updated_fields
        }))
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

        std::fs::remove_file(&full_path).mcp_err_ctx("Failed to delete file")?;

        // TODO: Trigger re-parsing via crucible_core::parser after note deletion

        json_success(serde_json::json!({
            "path": path,
            "status": "deleted"
        }))
    }

    #[tool(description = "List notes in a directory")]
    pub async fn list_notes(
        &self,
        params: Parameters<ListNotesParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let params = params.0;
        // LLMs sometimes send the literal string "null" instead of omitting the field
        let folder = params.folder.filter(|f| !f.is_empty() && f != "null");
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
}
