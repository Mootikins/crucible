//! List implementation helpers for `NoteTools::list_notes`.

use super::super::helpers::McpResultExt;
use super::super::utils::parse_yaml_frontmatter;
use super::NoteTools;
use crucible_core::storage::NoteStore;
use rmcp::model::CallToolResult;
use std::sync::Arc;
use walkdir::WalkDir;

impl NoteTools {
    /// List notes using `NoteStore` index
    pub(super) async fn list_notes_via_store(
        &self,
        note_store: &Arc<dyn NoteStore>,
        folder: Option<&str>,
        include_frontmatter: bool,
        recursive: bool,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        // MCP tools are scoped to the kiln they're serving. A plugin/agent
        // hosting MCP against this kiln gets workspace authority; cross-kiln
        // notes are not visible through this surface.
        let authority =
            crucible_core::storage::Scope::workspace(&self.kiln_path).unwrap_or_else(|_| {
                crucible_core::storage::Scope::workspace_unchecked(&self.kiln_path)
            });
        let all_notes = note_store
            .list(&authority)
            .await
            .mcp_err_ctx("Failed to list notes from store")?;

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

        super::super::helpers::json_success(serde_json::json!({
            "notes": notes,
            "folder": folder,
            "count": notes.len(),
            "recursive": recursive,
            "source": "index"
        }))
    }

    /// List notes using filesystem scanning (fallback)
    ///
    /// This function is async for API consistency with the `NoteStore` path,
    /// even though filesystem operations are synchronous.
    #[allow(clippy::unused_async)]
    pub(super) async fn list_notes_via_filesystem(
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
            for entry in std::fs::read_dir(search_path).mcp_err_ctx("Failed to read directory")? {
                let entry = entry.mcp_err_ctx("Failed to read entry")?;
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

        super::super::helpers::json_success(serde_json::json!({
            "notes": notes,
            "folder": folder,
            "count": notes.len(),
            "recursive": recursive
        }))
    }
}
