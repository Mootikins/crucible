//! Note CRUD operations tools
//!
//! The filesystem is the source of truth: every write, and every read of a
//! note's content, goes to disk. The two metadata reads (`read_metadata`,
//! `list_notes`) answer from the kiln's index when the index has a row for the
//! note, and from disk when it does not. An index row for a file that is gone
//! is stale, so the file has to exist either way.

#![allow(missing_docs)]

mod helpers;
mod list;
mod params;

#[cfg(test)]
mod tests;

use super::containment::RootSet;
use super::fs_scope::FsScope;
use super::helpers::{json_success, McpResultExt};
use super::utils::parse_yaml_frontmatter;
use helpers::{
    extract_content_without_frontmatter, resolve_note_write, serialize_frontmatter_to_yaml,
};

use crucible_core::storage::note_store::NoteRecord;
use crucible_core::traits::KnowledgeRepository;
/// How a note name becomes a note path, and the rule that says the result has
/// to be a note — re-exported for the other note-writing sink in this crate
/// (`acp::tools::ToolExecutor`), which had its own hand-rolled copy of the
/// first and none of the second. One definition, because two would drift.
use helpers::ensure_md_suffix;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{model::CallToolResult, tool, tool_router};
use std::sync::Arc;

pub use params::{
    CreateNoteParams, DeleteNoteParams, ListNotesParams, ReadMetadataParams, ReadNoteParams,
    UpdateNoteParams,
};

#[derive(Clone)]
#[allow(missing_docs)]
pub struct NoteTools {
    /// The kiln as a capability rather than a path: kiln-relative names only,
    /// the kiln is a boundary of its own, the control directory is protected,
    /// and every reachability question — including the ones a walk asks — goes
    /// to the session's root set. Holding a `String` here is what let
    /// `read_note` hand over a transcript `read_file` refused.
    scope: FsScope,
    /// The kiln's index. Required, not optional: a session without a kiln
    /// gets a repository that knows no notes, and every read goes to disk.
    index: Arc<dyn KnowledgeRepository>,
}

impl NoteTools {
    #[allow(missing_docs)]
    #[must_use]
    pub fn new(kiln_path: String, index: Arc<dyn KnowledgeRepository>) -> Self {
        Self {
            scope: FsScope::kiln(kiln_path, RootSet::Ambient),
            index,
        }
    }

    /// The index row for a kiln-relative path, or `None` when the index has
    /// none. An index that fails to answer is the same as an index with no
    /// row: the file is the source of truth, and disk still answers.
    pub(super) async fn indexed(&self, relative: &std::path::Path) -> Option<NoteRecord> {
        let key = relative.to_string_lossy();
        match self.index.get_note_by_path(&key).await {
            Ok(row) => row,
            Err(e) => {
                tracing::debug!(path = %key, error = %e, "index lookup failed; reading disk");
                None
            }
        }
    }

    /// Contain these tools to the session's roots. Without it the scope knows
    /// only its own kiln, which is not enough whenever the kiln ENCLOSES
    /// something the session may not read.
    #[must_use]
    pub(crate) fn with_containment(mut self, containment: RootSet) -> Self {
        self.scope = self.scope.with_containment(containment);
        self
    }

    pub(super) fn scope(&self) -> &FsScope {
        &self.scope
    }

    /// A bare filename is looked up by walking the kiln — so the walk is the
    /// thing that has to be contained. Filtering only the caller's string would
    /// leave `read_note {"path": "session.jsonl"}` finding a transcript it was
    /// never allowed to name.
    fn resolve_note_name(&self, path: &str) -> Result<String, rmcp::ErrorData> {
        if path.contains('/') || path.contains('\\') {
            return Ok(path.to_string());
        }

        let direct = self.scope.resolve(path)?;
        if direct.as_path().is_file() {
            return Ok(path.to_string());
        }

        let root = self.scope.resolve("")?;
        let found = self
            .scope
            .walk_files(&root)
            .find(|entry| entry.file_name() == std::ffi::OsStr::new(path))
            .ok_or_else(|| {
                rmcp::ErrorData::invalid_params(format!("File not found: {path}"), None)
            })?;

        Ok(self
            .scope
            .relativize(found.path())
            .to_string_lossy()
            .to_string())
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

        // Security: containment, the protected set, and the extension rule —
        // a note tool creates files, so it is a write path like `write_file`
        // and answers to the same hardcoded deny, and what it creates has to
        // be a note or `ToolSurface::Daemon` is a lie about its reach.
        let full_path = resolve_note_write(&self.scope, &path)?;

        // Build final content with optional frontmatter
        let final_content = if let Some(fm) = frontmatter {
            let fm_str = serialize_frontmatter_to_yaml(&fm).mcp_err()?;
            format!("{fm_str}{content}")
        } else {
            content
        };

        std::fs::write(full_path.as_path(), &final_content).mcp_err_ctx("Failed to write file")?;

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
        let full_path = self.scope.resolve(&resolved_path)?;

        if !full_path.exists() {
            return Err(rmcp::ErrorData::invalid_params(
                format!("File not found: {path}"),
                None,
            ));
        }

        let content =
            std::fs::read_to_string(full_path.as_path()).mcp_err_ctx("Failed to read file")?;

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
        let full_path = self.scope.resolve(&path)?;

        if !full_path.exists() {
            return Err(rmcp::ErrorData::invalid_params(
                format!("File not found: {path}"),
                None,
            ));
        }

        if let Some(row) = self
            .indexed(self.scope.relativize(full_path.as_path()))
            .await
        {
            return json_success(serde_json::json!({
                "path": path,
                "frontmatter": indexed_frontmatter(&row),
                "stats": {
                    "links_count": row.links_to.len(),
                    "tags_count": row.tags.len(),
                    "has_embedding": row.has_embedding(),
                },
                "modified": indexed_modified(&row),
                "source": "index"
            }));
        }

        let content =
            std::fs::read_to_string(full_path.as_path()).mcp_err_ctx("Failed to read file")?;

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
        let metadata = std::fs::metadata(full_path.as_path()).ok();
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
            "modified": modified,
            "source": "disk"
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

        // Security: containment, the protected set, and the extension rule.
        let full_path = resolve_note_write(&self.scope, &path)?;

        if !full_path.exists() {
            return Err(rmcp::ErrorData::invalid_params(
                format!("File not found: {path}"),
                None,
            ));
        }

        // Read existing file
        let existing_content =
            std::fs::read_to_string(full_path.as_path()).mcp_err_ctx("Failed to read file")?;

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

        std::fs::write(full_path.as_path(), &final_file_content)
            .mcp_err_ctx("Failed to update file")?;

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

        // Security: containment, the protected set, and the extension rule —
        // `delete_note` is the destructive half of the same authority, so a
        // path it may not write is a path it may not remove.
        let full_path = resolve_note_write(&self.scope, &path)?;

        if !full_path.exists() {
            return Err(rmcp::ErrorData::invalid_params(
                format!("File not found: {path}"),
                None,
            ));
        }

        std::fs::remove_file(full_path.as_path()).mcp_err_ctx("Failed to delete file")?;

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
        let search_path = self.scope.resolve_folder(folder.as_deref())?;

        if !search_path.exists() {
            // The caller supplied `folder`; the kiln root it resolves against
            // is what it did not. Reporting `search_path` turned a missing
            // folder into an oracle for the kiln's location.
            return Err(rmcp::ErrorData::invalid_params(
                format!("Folder not found: {}", folder.as_deref().unwrap_or(".")),
                None,
            ));
        }

        self.list_notes_via_filesystem(
            &search_path,
            folder.as_deref(),
            include_frontmatter,
            recursive,
        )
        .await
    }
}

/// The frontmatter an index row stands for: the stored properties, with the
/// title and tags the indexer lifted out of them put back.
pub(super) fn indexed_frontmatter(row: &NoteRecord) -> serde_json::Value {
    let mut frontmatter = serde_json::json!({
        "title": row.title,
        "tags": row.tags,
    });
    if let Some(obj) = frontmatter.as_object_mut() {
        for (k, v) in &row.properties {
            obj.insert(k.clone(), v.clone());
        }
    }
    frontmatter
}

/// The row's update time as Unix seconds, in the same unit the disk path
/// reports `mtime`.
pub(super) fn indexed_modified(row: &NoteRecord) -> Option<u64> {
    row.updated_at.timestamp().try_into().ok()
}
