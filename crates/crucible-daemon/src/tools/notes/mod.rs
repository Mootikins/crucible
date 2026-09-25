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
mod propose;

#[cfg(test)]
mod tests;

use super::containment::RootSet;
use super::fs_scope::FsScope;
use super::helpers::{json_success, McpResultExt};
use super::utils::parse_yaml_frontmatter;
use helpers::{
    extract_content_without_frontmatter, resolve_note_write, serialize_frontmatter_to_yaml,
};

use crate::file_write::{write_locked, LockedChange};
use crucible_core::file_write::ExpectedBase;
use crucible_core::note_edit::disk_hash;
use crucible_core::storage::note_store::NoteRecord;
use crucible_core::traits::KnowledgeRepository;
use helpers::ensure_md_suffix;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{model::CallToolResult, tool, tool_router};
use std::path::Path;
use std::sync::Arc;

pub use propose::{author_of, NoteWrites, TurnWriteMode};

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
    /// Where a note write goes. `None` for a tool set with no session, such
    /// as the daemon-global one: every write applies to the disk.
    writes: Option<NoteWrites>,
}

impl NoteTools {
    #[allow(missing_docs)]
    #[must_use]
    pub fn new(kiln_path: String, index: Arc<dyn KnowledgeRepository>) -> Self {
        Self {
            scope: FsScope::kiln(kiln_path, RootSet::Ambient),
            index,
            writes: None,
        }
    }

    /// Send the note writes of the session to `writes`. In a turn whose
    /// write mode is `propose`, a write becomes a proposal.
    #[must_use]
    pub fn with_writes(mut self, writes: NoteWrites) -> Self {
        self.writes = Some(writes);
        self
    }

    /// The proposal target of the current turn, or `None` when the write
    /// applies to the disk.
    fn proposing(&self) -> Option<&NoteWrites> {
        self.writes
            .as_ref()
            .filter(|w| w.mode() == crucible_core::types::WriteMode::Propose)
    }

    /// The root and the root-relative path that a proposal names for
    /// `full_path`.
    fn proposal_target(&self, full_path: &Path) -> (crucible_core::session::PhysicalRoot, String) {
        let root =
            crucible_core::session::PhysicalRoot::from_top_level(self.scope.canonical_anchor());
        let relative = self
            .scope
            .relativize(full_path)
            .to_string_lossy()
            .to_string();
        (root, relative)
    }

    /// Record a proposed write of `full_path`, and build the tool answer.
    /// The file on disk does not change.
    fn propose(
        &self,
        writes: &NoteWrites,
        path: &str,
        full_path: &Path,
        base: ExpectedBase,
        text: String,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let (root, relative) = self.proposal_target(full_path);
        let proposal = writes.propose(root, &relative, base, text)?;
        json_success(serde_json::json!({
            "path": path,
            "status": "proposed",
            "proposal": proposal.id,
            "message": "The note on disk did not change. The user accepts or rejects the proposal later."
        }))
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
        let _write = crate::file_write::lock(full_path.as_path()).await;

        // Build final content with optional frontmatter
        let final_content = if let Some(fm) = frontmatter {
            let fm_str = serialize_frontmatter_to_yaml(&fm).mcp_err()?;
            format!("{fm_str}{content}")
        } else {
            content
        };

        // In `propose` mode the base is `Absent`: accept must not replace a
        // file that another writer created after this proposal.
        if let Some(writes) = self.proposing() {
            return self.propose(
                writes,
                &path,
                full_path.as_path(),
                ExpectedBase::Absent,
                final_content,
            );
        }

        // `create_note` replaces an existing file with no check, as it did
        // before the checked write.
        let answer =
            write_note(full_path.as_path(), final_content, ExpectedBase::Unchecked).await?;

        // TODO: Trigger re-parsing via crucible_core::parser after note creation

        json_success(serde_json::json!({
            "path": path,
            "status": "created",
            "content_hash": answer["content_hash"]
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
        let _write = crate::file_write::lock(full_path.as_path()).await;

        if let Some(writes) = self.proposing() {
            // The disk does not hold an earlier proposed write of this turn,
            // so the update builds on the proposed text when there is one.
            // The store keeps the base of the first write of the path, and
            // it ignores the base of this write.
            let (root, relative) = self.proposal_target(full_path.as_path());
            let (text, base) = match writes.proposed_text(&root, &relative)? {
                Some(proposed) => {
                    let (text, _) = updated_text(&proposed, new_frontmatter, new_content)?;
                    (text, ExpectedBase::Unchecked)
                }
                None => {
                    let (text, base, _) =
                        updated_note(full_path.as_path(), &path, new_frontmatter, new_content)?;
                    (text, base)
                }
            };
            return self.propose(writes, &path, full_path.as_path(), base, text);
        }
        let (text, base, updated_fields) =
            updated_note(full_path.as_path(), &path, new_frontmatter, new_content)?;
        let answer = write_note(full_path.as_path(), text, base).await?;

        // TODO: Trigger re-parsing via crucible_core::parser after note update

        json_success(serde_json::json!({
            "path": path,
            "status": "updated",
            "updated_fields": updated_fields,
            "merged": answer["merged"],
            "content_hash": answer["content_hash"]
        }))
    }

    #[tool(description = "Delete a note from the kiln")]
    pub async fn delete_note(
        &self,
        params: Parameters<DeleteNoteParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        // A proposal holds new text, not a removal, so a delete cannot wait
        // for review.
        if self.proposing().is_some() {
            return Err(rmcp::ErrorData::invalid_request(
                "delete_note is not available in propose mode; ask the user to delete the note",
                None,
            ));
        }
        let params = params.0;
        let path = ensure_md_suffix(params.path);

        // Security: containment, the protected set, and the extension rule —
        // `delete_note` is the destructive half of the same authority, so a
        // path it may not write is a path it may not remove.
        let full_path = resolve_note_write(&self.scope, &path)?;
        let _write = crate::file_write::lock(full_path.as_path()).await;

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

/// Read the note at `full_path` and build the text that `update_note` writes.
/// The base is the text that this function read, so a write after an outside
/// edit merges with that edit. The caller holds `file_write::lock(full_path)`.
fn updated_note(
    full_path: &Path,
    path: &str,
    new_frontmatter: Option<serde_json::Value>,
    new_content: Option<String>,
) -> Result<(String, ExpectedBase, Vec<&'static str>), rmcp::ErrorData> {
    if !full_path.exists() {
        return Err(rmcp::ErrorData::invalid_params(
            format!("File not found: {path}"),
            None,
        ));
    }

    let existing_content = std::fs::read_to_string(full_path).mcp_err_ctx("Failed to read file")?;
    let (final_file_content, updated_fields) =
        updated_text(&existing_content, new_frontmatter, new_content)?;
    let base = ExpectedBase::Text {
        hash: disk_hash(&existing_content),
        text: existing_content,
    };
    Ok((final_file_content, base, updated_fields))
}

/// Build the text that `update_note` writes from the `existing` note text.
fn updated_text(
    existing_content: &str,
    new_frontmatter: Option<serde_json::Value>,
    new_content: Option<String>,
) -> Result<(String, Vec<&'static str>), rmcp::ErrorData> {
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
            let content = extract_content_without_frontmatter(existing_content);
            (Some(fm), content)
        }
        (None, Some(content)) => {
            // Update content only, preserve frontmatter
            updated_fields.push("content");
            let fm = parse_yaml_frontmatter(existing_content);
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
    Ok((final_file_content, updated_fields))
}

/// Write `text` through the daemon's checked write. A conflict or a refusal
/// becomes a tool error, and the disk does not change. The caller holds
/// `file_write::lock(full_path)`.
async fn write_note(
    full_path: &Path,
    text: String,
    base: ExpectedBase,
) -> Result<serde_json::Value, rmcp::ErrorData> {
    let answer = write_locked(full_path, LockedChange::Put(text), base)
        .await
        .mcp_err_ctx("Failed to write file")?;
    if answer["ok"] == true {
        return Ok(answer);
    }
    if let Some(message) = answer["message"].as_str() {
        return Err(rmcp::ErrorData::invalid_params(
            format!("Failed to write file: {message}"),
            None,
        ));
    }
    Err(rmcp::ErrorData::invalid_request(
        "The note changed on disk, and the change conflicts with this edit. \
         The note on disk did not change. Read the note again, then repeat the edit."
            .to_string(),
        Some(serde_json::json!({
            "current_hash": answer["current_hash"],
            "regions": answer["regions"],
        })),
    ))
}
