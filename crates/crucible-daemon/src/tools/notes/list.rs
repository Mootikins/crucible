//! List implementation helpers for `NoteTools::list_notes`.

use super::super::fs_scope::ContainedPath;
use super::super::helpers::McpResultExt;
use super::super::utils::parse_yaml_frontmatter;
use super::NoteTools;
use rmcp::model::CallToolResult;

impl NoteTools {
    /// List notes from the filesystem.
    ///
    /// The function is async because the tool router calls it as a future,
    /// although the filesystem operations are synchronous.
    #[allow(clippy::unused_async)]
    pub(super) async fn list_notes_via_filesystem(
        &self,
        search_path: &ContainedPath,
        folder: Option<&str>,
        include_frontmatter: bool,
        recursive: bool,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let mut notes = Vec::new();

        // Both branches enumerate through the scope, which drops (and, in the
        // recursive case, refuses to descend into) anything the session may not
        // read. A listing is a leak in its own right: the paths alone name
        // every session recorded under a kiln that encloses the sessions root.
        let entries: Vec<std::path::PathBuf> = if recursive {
            self.scope()
                .walk_files(search_path)
                .map(|entry| entry.into_path())
                .collect()
        } else {
            self.scope()
                .read_dir(search_path)
                .mcp_err_ctx("Failed to read directory")?
                .iter()
                .map(|path| path.as_path().to_path_buf())
                .filter(|path| path.is_file())
                .collect()
        };

        for path in entries {
            if !crucible_core::kiln::is_indexable_file(&path) {
                continue;
            }
            let metadata = std::fs::metadata(&path).ok();
            let modified = metadata
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs());

            let mut note_json = serde_json::json!({
                "path": self.scope().relativize(&path).to_string_lossy(),
                "size": metadata.as_ref().map_or(0, std::fs::Metadata::len),
                "modified": modified
            });

            if include_frontmatter {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    let frontmatter =
                        parse_yaml_frontmatter(&content).unwrap_or_else(|| serde_json::json!({}));
                    note_json["frontmatter"] = frontmatter;
                    note_json["word_count"] = serde_json::json!(content.split_whitespace().count());
                }
            }

            notes.push(note_json);
        }

        super::super::helpers::json_success(serde_json::json!({
            "notes": notes,
            "folder": folder,
            "count": notes.len(),
            "recursive": recursive
        }))
    }
}
